use cosmic_share_browser::config::Config;
use cosmic_share_browser::firewall;
use dav_server::{fakels::FakeLs, localfs::LocalFs, DavHandler};
use std::sync::Arc;
use tokio::sync::Notify;

// Tracks a running server instance so the run-loop can tear it down cleanly.
struct ServerHandle {
    port: u16,
    stop: Arc<Notify>,
}

#[tokio::main]
async fn main() {
    let global_shutdown = Arc::new(Notify::new());
    let gs = global_shutdown.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler failed");
        let mut intr = signal(SignalKind::interrupt()).expect("SIGINT handler failed");
        tokio::select! {
            _ = term.recv() => {}
            _ = intr.recv() => {}
        }
        gs.notify_waiters();
    });

    run(global_shutdown).await;
}

// ---------------------------------------------------------------------------
// Port-file helpers
// ---------------------------------------------------------------------------

fn write_port_file(port: u16) {
    let path = Config::runtime_port_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Err(e) = std::fs::write(&path, port.to_string()) {
        eprintln!("cosmic-share-daemon: failed to write port file: {}", e);
    } else {
        eprintln!("cosmic-share-daemon: wrote port {} to {:?}", port, path);
    }
}

fn remove_port_file() {
    let path = Config::runtime_port_path();
    if path.exists() {
        std::fs::remove_file(&path).ok();
        eprintln!("cosmic-share-daemon: removed port file");
    }
}

// ---------------------------------------------------------------------------
// Server lifecycle
// ---------------------------------------------------------------------------

/// Stop a running server: signal tasks, close firewall port, remove port file.
async fn stop_server(handle: &mut Option<ServerHandle>) {
    if let Some(h) = handle.take() {
        h.stop.notify_waiters();

        // Best-effort firewall cleanup (no pkexec — we're headless)
        if !firewall::deny_port_direct(h.port).await {
            eprintln!(
                "cosmic-share-daemon: could not close firewall port {} \
                 (GUI can clean up via pkexec)",
                h.port
            );
        }

        remove_port_file();

        // Let connections drain
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        eprintln!("cosmic-share-daemon: server stopped (was port {})", h.port);
    }
}

/// Bind, open firewall, advertise via avahi, serve WebDAV.
async fn spawn_server(config: &Config) -> Option<ServerHandle> {
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;
    use std::convert::Infallible;

    let dir = config.shared_dir.clone();
    let service_name = config.service_name.clone();
    let read_only = config.read_only;

    // --- bind ---------------------------------------------------------------
    let listener = match tokio::net::TcpListener::bind("0.0.0.0:0").await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cosmic-share-daemon: failed to bind: {}", e);
            return None;
        }
    };
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => {
            eprintln!("cosmic-share-daemon: could not read port: {}", e);
            return None;
        }
    };

    eprintln!("cosmic-share-daemon: serving {} on port {} ({})", dir, port,
        if read_only { "read-only" } else { "read-write" });

    // --- firewall -----------------------------------------------------------
    if !firewall::allow_port_direct(port).await {
        eprintln!(
            "cosmic-share-daemon: could not open firewall port {} without pkexec \
             (GUI can open it interactively)",
            port
        );
    }

    // --- port file ----------------------------------------------------------
    write_port_file(port);

    let stop = Arc::new(Notify::new());

    // --- avahi --------------------------------------------------------------
    let stop_avahi = stop.clone();
    let port_str = port.to_string();
    tokio::spawn(async move {
        let child = tokio::process::Command::new("avahi-publish-service")
            .args([&service_name, "_webdav._tcp", &port_str, "u=/", "path=/"])
            .spawn();
        if let Ok(mut child) = child {
            tokio::select! {
                _ = stop_avahi.notified() => { child.kill().await.ok(); }
                _ = child.wait() => {}
            }
        }
    });

    // --- webdav -------------------------------------------------------------
    let dav_handler = DavHandler::builder()
        .filesystem(LocalFs::new(&dir, false, false, false))
        .locksystem(FakeLs::new())
        .build_handler();

    let stop_http = stop.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_http.notified() => break,
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            let dav = dav_handler.clone();
                            tokio::spawn(async move {
                                let io = TokioIo::new(stream);
                                let svc = service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                                    let dav = dav.clone();
                                    async move {
                                        if read_only {
                                            let m = req.method();
                                            if m == hyper::Method::PUT
                                                || m == hyper::Method::DELETE
                                                || m == "MKCOL"
                                                || m == "MOVE"
                                                || m == "COPY"
                                                || m == "PROPPATCH"
                                                || m == "LOCK"
                                                || m == "UNLOCK"
                                            {
                                                return Ok::<_, Infallible>(
                                                    hyper::Response::builder()
                                                        .status(405)
                                                        .body(dav_server::body::Body::from(
                                                            "Read-only share",
                                                        ))
                                                        .unwrap(),
                                                );
                                            }
                                        }
                                        Ok::<_, Infallible>(dav.handle(req).await)
                                    }
                                });
                                http1::Builder::new().serve_connection(io, svc).await.ok();
                            });
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    Some(ServerHandle { port, stop })
}

// ---------------------------------------------------------------------------
// Main run-loop
// ---------------------------------------------------------------------------

async fn run(global_shutdown: Arc<Notify>) {
    let mut last_mtime = Config::mtime();
    let mut server: Option<ServerHandle> = None;
    let mut current_config: Option<Config> = None;

    let config_path = Config::path();
    eprintln!("cosmic-share-daemon: config path = {:?}", config_path);
    eprintln!("cosmic-share-daemon: config exists = {}", config_path.exists());

    let initial_config = Config::load();
    eprintln!("cosmic-share-daemon: enabled = {}", initial_config.enabled);
    eprintln!("cosmic-share-daemon: shared_dir = {}", initial_config.shared_dir);

    if initial_config.enabled {
        server = spawn_server(&initial_config).await;
        current_config = Some(initial_config);
    } else {
        eprintln!("cosmic-share-daemon: enabled=false, waiting for config change");
    }

    loop {
        tokio::select! {
            _ = global_shutdown.notified() => {
                stop_server(&mut server).await;
                eprintln!("cosmic-share-daemon: shutting down");
                break;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(3)) => {
                let new_mtime = Config::mtime();
                if new_mtime == last_mtime { continue; }
                last_mtime = new_mtime;

                let config = Config::load();

                let needs_restart = match &current_config {
                    None => config.enabled,
                    Some(c) => config.enabled && (c.shared_dir != config.shared_dir
                        || c.service_name != config.service_name
                        || c.read_only != config.read_only),
                };

                if !config.enabled {
                    if server.is_some() {
                        stop_server(&mut server).await;
                        eprintln!("cosmic-share-daemon: sharing disabled");
                    }
                    current_config = Some(config);
                } else if needs_restart || server.is_none() {
                    // Stop old server first (closes old firewall port)
                    stop_server(&mut server).await;
                    // Start new server (opens new firewall port)
                    server = spawn_server(&config).await;
                    current_config = Some(config);
                }
            }
        }
    }
}
