use cosmic::app::{Core, Task};
use cosmic::iced::window;
use cosmic::iced::Alignment;
use cosmic::{executor, widget, Application, Element};
use cosmic_share_browser::config::Config;
use cosmic_share_browser::firewall::{self, Firewall};
use std::collections::HashSet;

const APP_ID: &str = "org.cachyos.CosmicShareBrowser";

const SERVICE_FILE: &str = "[Unit]
Description=COSMIC Share Browser - WebDAV sharing daemon
After=network.target avahi-daemon.service

[Service]
ExecStart=%h/.local/bin/cosmic-share-daemon
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
";

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<App>(())
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum DaemonStatus {
    Unknown,
    NotInstalled,
    Stopped,
    Running,
}

impl DaemonStatus {
    fn label(&self) -> &str {
        match self {
            Self::Unknown => "Checking...",
            Self::NotInstalled => "Not installed",
            Self::Stopped => "○ Stopped",
            Self::Running => "● Running",
        }
    }
}

#[derive(Debug, Clone)]
struct Share {
    hostname: String,
    port: u16,
    uri: String,
}

impl Share {
    fn new(hostname: String, port: u16) -> Self {
        let uri = format!("dav://{}:{}/", hostname, port);
        Self { hostname, port, uri }
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct App {
    core: Core,
    config: Config,
    dir_input: String,
    config_dirty: bool,
    daemon_status: DaemonStatus,
    daemon_port: Option<u16>,
    firewall: Firewall,
    firewall_open: bool,
    server_status: String,
    // client
    shares: Vec<Share>,
    mounted: HashSet<String>,
    scanning: bool,
    client_status: String,
    // applet
    popup: Option<window::Id>,
    icon_name: String,
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Message {
    // applet
    TogglePopup,
    PopupClosed,
    // startup
    Init,
    InitResult {
        daemon: DaemonStatus,
        firewall: Firewall,
        port: Option<u16>,
    },
    // config
    DirChanged(String),
    SaveConfig,
    ConfigSaved,
    ToggleReadOnly,
    // daemon
    InstallAndStart,
    InstallResult(bool),
    ToggleEnabled,
    ToggleResult(bool),
    StartDaemon,
    StopDaemon,
    DaemonStarted(bool),
    DaemonStopped(bool),
    // port / firewall
    PortRead(Option<u16>),
    ConfigPortReady(Option<u16>),
    InitFirewallDone(u16, bool),
    OpenFirewall,
    CloseFirewall,
    FirewallResult(bool, bool),

    // client
    Scan,
    ScanComplete(Vec<Share>),
    MountedRefreshed(HashSet<String>),
    Mount(String),
    Unmount(String),
    MountDone(String, bool),
    UnmountDone(String, bool),
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        let config = Config::load();
        let dir = config.shared_dir.clone();
        let app = Self {
            core,
            config,
            dir_input: dir,
            config_dirty: false,
            daemon_status: DaemonStatus::Unknown,
            daemon_port: None,
            firewall: Firewall::None,
            firewall_open: false,
            server_status: String::from("Checking..."),
            shares: Vec::new(),
            mounted: HashSet::new(),
            scanning: false,
            client_status: String::new(),
            popup: None,
            icon_name: String::from("network-server-symbolic"),
        };
        let task = cosmic::task::future(async { Message::Init });
        (app, task)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        if self.popup == Some(id) {
            Some(Message::PopupClosed)
        } else {
            None
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {

            // ── Applet popup ──────────────────────────────────────────────
            Message::TogglePopup => {
                if let Some(p) = self.popup.take() {
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(p);
                }
                let new_id = window::Id::unique();
                self.popup.replace(new_id);
                let popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    new_id,
                    Some((420, 600)),
                    None,
                    None,
                );
                cosmic::iced::platform_specific::shell::commands::popup::get_popup(popup_settings)
            }
            Message::PopupClosed => {
                self.popup = None;
                Task::none()
            }

            // ── Init ──────────────────────────────────────────────────────
            Message::Init => {
                cosmic::task::future(async {
                    let daemon = check_daemon_status().await;
                    let firewall = Firewall::detect();
                    let port = Config::read_daemon_port();
                    Message::InitResult { daemon, firewall, port }
                })
            }

            Message::InitResult { daemon, firewall, port } => {
                self.daemon_status = daemon.clone();
                self.firewall = firewall.clone();
                self.daemon_port = port;
                self.update_icon();
                self.server_status = self.build_server_status();

                // Auto-open firewall if daemon is running with a known port
                if let Some(new_port) = port {
                    if firewall != Firewall::None && self.config.enabled {
                        let old_port = Config::read_last_port();
                        self.server_status = format!("Opening firewall port {}...", new_port);
                        return cosmic::task::future(async move {
                            let ok = firewall::swap_port(old_port, new_port).await;
                            Message::InitFirewallDone(new_port, ok)
                        });
                    }
                }

                cosmic::task::future(async {
                    Message::ScanComplete(discover_shares().await)
                })
            }

            // ── Config ────────────────────────────────────────────────────
            Message::DirChanged(s) => {
                self.dir_input = s;
                self.config_dirty = true;
                Task::none()
            }

            Message::SaveConfig => {
                self.config.shared_dir = self.dir_input.clone();
                let config = self.config.clone();
                self.server_status = String::from("Applying changes...");
                cosmic::task::future(async move {
                    config.save().ok();
                    systemctl_user(&["restart", "cosmic-share-daemon"]).await;
                    Message::ConfigSaved
                })
            }
            Message::ConfigSaved => {
                self.config_dirty = false;
                self.daemon_port = None;
                self.server_status = String::from("Restarting daemon...");
                cosmic::task::future(async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    Message::ConfigPortReady(Config::read_daemon_port())
                })
            }

            Message::ToggleReadOnly => {
                self.config.read_only = !self.config.read_only;
                let config = self.config.clone();
                let label = if self.config.read_only { "read-only" } else { "read-write" };
                self.server_status = format!("Switching to {}...", label);
                cosmic::task::future(async move {
                    config.save().ok();
                    // Daemon detects config change within 3s and restarts
                    Message::ConfigSaved
                })
            }

            // ── Daemon install ────────────────────────────────────────────
            Message::InstallAndStart => {
                let config = self.config.clone();
                self.server_status = String::from("Installing service...");
                cosmic::task::future(async move {
                    config.save().ok();
                    let ok = install_service().await;
                    Message::InstallResult(ok)
                })
            }
            Message::InstallResult(ok) => {
                if ok {
                    self.daemon_status = DaemonStatus::Running;
                    self.config.enabled = true;
                    self.config.save().ok();
                    self.update_icon();
                    self.server_status = self.build_server_status();
                    cosmic::task::future(async {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        Message::ConfigPortReady(Config::read_daemon_port())
                    })
                } else {
                    self.server_status = String::from(
                        "Install failed. Is cosmic-share-daemon in ~/.local/bin/?",
                    );
                    Task::none()
                }
            }

            // ── Daemon toggle enabled ─────────────────────────────────────
            Message::ToggleEnabled => {
                self.config.enabled = !self.config.enabled;
                let enabled = self.config.enabled;
                let config = self.config.clone();
                self.server_status = if enabled {
                    String::from("Enabling sharing...")
                } else {
                    String::from("Disabling sharing...")
                };
                cosmic::task::future(async move {
                    config.save().ok();
                    Message::ToggleResult(enabled)
                })
            }
            Message::ToggleResult(enabled) => {
                self.update_icon();
                self.server_status = if enabled {
                    String::from("Sharing enabled. Daemon will start within 5s.")
                } else {
                    String::from("Sharing disabled. Daemon will stop within 5s.")
                };
                cosmic::task::future(async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(4)).await;
                    Message::PortRead(Config::read_daemon_port())
                })
            }

            // ── Daemon start/stop ─────────────────────────────────────────
            Message::StartDaemon => {
                self.server_status = String::from("Starting daemon...");
                cosmic::task::future(async {
                    let ok = systemctl_user(&["start", "cosmic-share-daemon"]).await;
                    Message::DaemonStarted(ok)
                })
            }
            Message::StopDaemon => {
                self.server_status = String::from("Stopping daemon...");
                cosmic::task::future(async {
                    let ok = systemctl_user(&["stop", "cosmic-share-daemon"]).await;
                    Message::DaemonStopped(ok)
                })
            }
            Message::DaemonStarted(ok) => {
                if ok {
                    self.daemon_status = DaemonStatus::Running;
                    self.update_icon();
                    self.server_status = self.build_server_status();
                    cosmic::task::future(async {
                        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        Message::PortRead(Config::read_daemon_port())
                    })
                } else {
                    self.server_status = String::from("Failed to start daemon.");
                    Task::none()
                }
            }
            Message::DaemonStopped(ok) => {
                if ok {
                    self.daemon_status = DaemonStatus::Stopped;
                    self.daemon_port = None;
                    self.firewall_open = false;
                    self.update_icon();
                    self.server_status = self.build_server_status();
                } else {
                    self.server_status = String::from("Failed to stop daemon.");
                }
                Task::none()
            }

            // ── Port / Firewall ───────────────────────────────────────────
            Message::PortRead(port) => {
                self.daemon_port = port;
                self.server_status = self.build_server_status();
                Task::none()
            }
            Message::InitFirewallDone(port, ok) => {
                if ok {
                    self.firewall_open = true;
                    Config::write_last_port(port);
                    self.server_status = format!("Port {} opened.", port);
                } else {
                    self.server_status = String::from(
                        "Auto-open failed. Click Open Port.",
                    );
                }
                // Now kick off client scan
                cosmic::task::future(async {
                    Message::ScanComplete(discover_shares().await)
                })
            }
            Message::ConfigPortReady(port) => {
                self.daemon_port = port;
                self.daemon_status = DaemonStatus::Running;
                self.update_icon();
                if let Some(p) = port {
                    let old_port = Config::read_last_port();
                    self.server_status = format!("Opening firewall port {}...", p);
                    cosmic::task::future(async move {
                        let ok = firewall::swap_port(old_port, p).await;
                        Message::FirewallResult(true, ok)
                    })
                } else {
                    self.server_status = String::from(
                        "Daemon restarted but port not yet available.",
                    );
                    Task::none()
                }
            }

            Message::OpenFirewall => {
                if let Some(port) = self.daemon_port {
                    self.server_status = format!("Opening firewall port {}...", port);
                    cosmic::task::future(async move {
                        let ok = firewall::allow_port(port).await;
                        Message::FirewallResult(true, ok)
                    })
                } else {
                    self.server_status = String::from(
                        "No port known — is the daemon running?",
                    );
                    Task::none()
                }
            }
            Message::CloseFirewall => {
                if let Some(port) = self.daemon_port {
                    self.server_status = format!("Closing firewall port {}...", port);
                    cosmic::task::future(async move {
                        let ok = firewall::deny_port(port).await;
                        Message::FirewallResult(false, ok)
                    })
                } else {
                    Task::none()
                }
            }
            Message::FirewallResult(opened, ok) => {
                if ok {
                    self.firewall_open = opened;
                    if opened {
                        if let Some(p) = self.daemon_port {
                            Config::write_last_port(p);
                        }
                        self.server_status = format!(
                            "Firewall port {} opened.",
                            self.daemon_port.unwrap_or(0),
                        );
                    } else {
                        Config::clear_last_port();
                        self.server_status = String::from("Firewall port closed.");
                    }
                } else {
                    self.server_status = String::from(
                        "Firewall change failed — check permissions.",
                    );
                }
                Task::none()
            }

            // ── Client ────────────────────────────────────────────────────
            Message::Scan => {
                self.scanning = true;
                self.client_status = String::from("Scanning...");
                cosmic::task::future(async {
                    Message::ScanComplete(discover_shares().await)
                })
            }
            Message::ScanComplete(shares) => {
                self.scanning = false;
                let count = shares.len();
                self.shares = shares;
                self.client_status = if count == 0 {
                    String::from("No shares found.")
                } else {
                    format!("Found {} share(s).", count)
                };
                cosmic::task::future(async {
                    Message::MountedRefreshed(get_mounted().await)
                })
            }
            Message::MountedRefreshed(mounted) => {
                self.mounted = mounted;
                Task::none()
            }
            Message::Mount(uri) => {
                let u = uri.clone();
                self.client_status = format!("Mounting {}...", uri);
                cosmic::task::future(async move {
                    Message::MountDone(u.clone(), mount_share(&u).await)
                })
            }
            Message::Unmount(uri) => {
                let u = uri.clone();
                self.client_status = format!("Unmounting {}...", uri);
                cosmic::task::future(async move {
                    Message::UnmountDone(u.clone(), unmount_share(&u).await)
                })
            }
            Message::MountDone(uri, ok) => {
                if ok { self.mounted.insert(uri.clone()); }
                self.client_status = if ok {
                    format!("Mounted: {}", uri)
                } else {
                    format!("Failed to mount: {}", uri)
                };
                Task::none()
            }
            Message::UnmountDone(uri, ok) => {
                if ok { self.mounted.remove(&uri); }
                self.client_status = if ok {
                    format!("Unmounted: {}", uri)
                } else {
                    format!("Failed to unmount: {}", uri)
                };
                Task::none()
            }
        }
    }

    // ── Panel icon ────────────────────────────────────────────────────────
    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button(&self.icon_name)
            .on_press_down(Message::TogglePopup)
            .into()
    }

    // ── Popup content ─────────────────────────────────────────────────────
    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.popup != Some(id) {
            return widget::text::body("").into();
        }

        let content = self.build_popup_content();

        self.core
            .applet
            .popup_container(content)
            .into()
    }
}

// ---------------------------------------------------------------------------
// View helpers
// ---------------------------------------------------------------------------

impl App {
    fn update_icon(&mut self) {
        self.icon_name = match (&self.daemon_status, self.config.enabled) {
            (DaemonStatus::Running, true) => "folder-remote-symbolic",
            _ => "network-server-symbolic",
        }
        .to_string();
    }

    fn build_server_status(&self) -> String {
        let mode = if self.config.read_only { "read-only" } else { "read-write" };
        match self.daemon_status {
            DaemonStatus::Running if self.config.enabled => {
                match self.daemon_port {
                    Some(port) => format!(
                        "Sharing {} on port {} ({}). Firewall: {}.",
                        self.config.shared_dir, port, mode, self.firewall.name(),
                    ),
                    None => format!(
                        "Sharing {} (port pending, {}). Firewall: {}.",
                        self.config.shared_dir, mode, self.firewall.name(),
                    ),
                }
            }
            DaemonStatus::Running => String::from("Daemon running, sharing disabled."),
            DaemonStatus::Stopped => String::from("Daemon stopped."),
            DaemonStatus::NotInstalled => String::from("Service not installed."),
            DaemonStatus::Unknown => String::from("Checking..."),
        }
    }

    fn build_popup_content(&self) -> Element<'_, Message> {
        let mut content = widget::Column::new().spacing(12).padding(16);

        // ── Server section ────────────────────────────────────────────
        content = match self.daemon_status {
            DaemonStatus::Unknown => {
                content
                    .push(widget::text::title4("Share My Files"))
                    .push(widget::text::body("Checking service status..."))
            }
            DaemonStatus::NotInstalled => {
                content
                    .push(widget::text::title4("Share My Files"))
                    .push(widget::text::body("Service not installed."))
                    .push(
                        widget::text_input("Directory (e.g. ~/Public)", &self.dir_input)
                            .on_input(Message::DirChanged)
                            .width(cosmic::iced::Length::Fill),
                    )
                    .push(
                        widget::button::suggested("Install & Start Service")
                            .on_press(Message::InstallAndStart),
                    )
            }
            DaemonStatus::Stopped | DaemonStatus::Running => {
                let is_running = self.daemon_status == DaemonStatus::Running;
                let is_enabled = self.config.enabled;

                // Status + controls row
                let daemon_btn = if is_running {
                    widget::button::destructive("Stop")
                        .on_press(Message::StopDaemon)
                } else {
                    widget::button::standard("Start")
                        .on_press(Message::StartDaemon)
                };

                let toggle_btn = if is_enabled {
                    widget::button::destructive("Disable")
                        .on_press(Message::ToggleEnabled)
                } else {
                    widget::button::suggested("Enable")
                        .on_press(Message::ToggleEnabled)
                };

                let header = widget::Row::new()
                    .push(widget::text::title4("Share My Files"))
                    .push(widget::Space::new().width(cosmic::iced::Length::Fill))
                    .push(widget::text::caption(self.daemon_status.label()))
                    .align_y(Alignment::Center)
                    .spacing(6);

                let controls = widget::Row::new()
                    .push(daemon_btn)
                    .push(toggle_btn)
                    .push(if self.config.read_only {
                        widget::button::standard("Read-Only")
                            .on_press(Message::ToggleReadOnly)
                    } else {
                        widget::button::destructive("Read-Write")
                            .on_press(Message::ToggleReadOnly)
                    })
                    .spacing(6);

                let dir_row = widget::Row::new()
                    .push(widget::text::body("Dir"))
                    .push(
                        widget::text_input("~/Public", &self.dir_input)
                            .on_input(Message::DirChanged)
                            .width(cosmic::iced::Length::Fill),
                    )
                    .align_y(Alignment::Center)
                    .spacing(8);

                let save_btn = if self.config_dirty {
                    widget::button::suggested("Save & Apply")
                        .on_press(Message::SaveConfig)
                } else {
                    widget::button::standard("Save & Apply")
                };

                // Port + firewall
                let port_label = match self.daemon_port {
                    Some(p) => format!("Port {} · {}", p, self.firewall.name()),
                    None if is_running => format!("Port pending · {}", self.firewall.name()),
                    None => format!("{}", self.firewall.name()),
                };

                let mut info_row = widget::Row::new()
                    .push(widget::text::caption(port_label))
                    .push(widget::Space::new().width(cosmic::iced::Length::Fill))
                    .align_y(Alignment::Center)
                    .spacing(6);

                if is_running && self.daemon_port.is_some()
                    && self.firewall != Firewall::None
                {
                    let fw_btn = if self.firewall_open {
                        widget::button::destructive("Close Port")
                            .on_press(Message::CloseFirewall)
                    } else {
                        widget::button::suggested("Open Port")
                            .on_press(Message::OpenFirewall)
                    };
                    info_row = info_row.push(fw_btn);
                }

                content
                    .push(header)
                    .push(controls)
                    .push(dir_row)
                    .push(
                        widget::Row::new()
                            .push(save_btn)
                            .spacing(6),
                    )
                    .push(info_row)
            }
        };

        content = content.push(widget::text::caption(self.server_status.as_str()));

        // ── Divider ───────────────────────────────────────────────────
        content = content.push(widget::divider::horizontal::default());

        // ── Client section ────────────────────────────────────────────
        let scan_label = if self.scanning { "Scanning..." } else { "Scan" };
        let scan_btn = if self.scanning {
            widget::button::standard(scan_label)
        } else {
            widget::button::standard(scan_label).on_press(Message::Scan)
        };

        let client_header = widget::Row::new()
            .push(widget::text::title4("Network Shares"))
            .push(widget::Space::new().width(cosmic::iced::Length::Fill))
            .push(scan_btn)
            .align_y(Alignment::Center)
            .spacing(8);

        content = content.push(client_header);

        if self.shares.is_empty() && !self.scanning {
            content = content.push(widget::text::body("No shares discovered."));
        } else {
            let mut list = widget::list_column();
            for share in &self.shares {
                let is_mounted = self.mounted.contains(&share.uri);
                let action_btn = if is_mounted {
                    widget::button::destructive("Unmount")
                        .on_press(Message::Unmount(share.uri.clone()))
                } else {
                    widget::button::suggested("Mount")
                        .on_press(Message::Mount(share.uri.clone()))
                };
                let item = widget::Row::new()
                    .push(
                        widget::Column::new()
                            .push(widget::text::body(share.hostname.as_str()))
                            .push(widget::text::caption(format!(
                                "port {}", share.port
                            )))
                            .spacing(2),
                    )
                    .push(widget::Space::new().width(cosmic::iced::Length::Fill))
                    .push(action_btn)
                    .align_y(Alignment::Center)
                    .spacing(8);
                list = list.add(item);
            }
            content = content.push(widget::scrollable(list));
        }

        if !self.client_status.is_empty() {
            content = content.push(widget::text::caption(self.client_status.as_str()));
        }

        content.into()
    }
}

// ---------------------------------------------------------------------------
// Daemon management helpers
// ---------------------------------------------------------------------------

async fn check_daemon_status() -> DaemonStatus {
    if !Config::service_path().exists() {
        return DaemonStatus::NotInstalled;
    }
    let ok = systemctl_user(&["is-active", "cosmic-share-daemon"]).await;
    if ok { DaemonStatus::Running } else { DaemonStatus::Stopped }
}

async fn systemctl_user(args: &[&str]) -> bool {
    tokio::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

async fn install_service() -> bool {
    use std::path::PathBuf;

    let daemon_bin = find_daemon_binary();
    let Some(daemon_bin) = daemon_bin else {
        eprintln!("Could not find cosmic-share-daemon binary");
        return false;
    };

    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/home/user"));
    let local_bin = PathBuf::from(&home).join(".local/bin");
    tokio::fs::create_dir_all(&local_bin).await.ok();
    let dest = local_bin.join("cosmic-share-daemon");
    if tokio::fs::copy(&daemon_bin, &dest).await.is_err() {
        eprintln!("Failed to copy daemon binary");
        return false;
    }

    let service_dir = PathBuf::from(&home).join(".config/systemd/user");
    tokio::fs::create_dir_all(&service_dir).await.ok();
    let service_path = Config::service_path();
    if tokio::fs::write(&service_path, SERVICE_FILE).await.is_err() {
        eprintln!("Failed to write service file");
        return false;
    }

    systemctl_user(&["daemon-reload"]).await;
    systemctl_user(&["enable", "--now", "cosmic-share-daemon"]).await
}

fn find_daemon_binary() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe.parent()?.join("cosmic-share-daemon");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let candidate = std::path::PathBuf::from(home).join(".local/bin/cosmic-share-daemon");
    if candidate.exists() {
        return Some(candidate);
    }
    if let Ok(output) = std::process::Command::new("which")
        .arg("cosmic-share-daemon")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return Some(std::path::PathBuf::from(path));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Client helpers
// ---------------------------------------------------------------------------

async fn discover_shares() -> Vec<Share> {
    // Gather local IP addresses to filter out our own share
    // Uses `ip -o addr show` (iproute2) — works on every Linux distro
    let local_ips: HashSet<String> = tokio::process::Command::new("ip")
        .args(["-o", "addr", "show"])
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|line| {
                    // Format: "2: wlan0  inet 192.168.4.64/24 ..."
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    parts.get(3).map(|cidr| {
                        cidr.split('/').next().unwrap_or("").to_string()
                    })
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let output = tokio::process::Command::new("avahi-browse")
        .args(["--terminate", "--resolve", "--parsable", "_webdav._tcp"])
        .output()
        .await;
    match output {
        Ok(out) => parse_avahi_output(&String::from_utf8_lossy(&out.stdout), &local_ips),
        Err(_) => Vec::new(),
    }
}

fn parse_avahi_output(output: &str, local_ips: &HashSet<String>) -> Vec<Share> {
    let mut shares = Vec::new();
    let mut seen = HashSet::new();
    for line in output.lines() {
        if !line.starts_with('=') { continue; }
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 10 { continue; }
        let mut hostname = parts[6].to_string();
        if hostname.ends_with('.') { hostname.pop(); }
        let port: u16 = parts[8].parse().unwrap_or(0);
        if hostname.is_empty() || port == 0 { continue; }
        // Skip this machine's own share (compare resolved address)
        let addr = parts[7];
        if local_ips.contains(addr) { continue; }
        let key = format!("{}:{}", hostname, port);
        if seen.insert(key) {
            shares.push(Share::new(hostname, port));
        }
    }
    shares
}

async fn get_mounted() -> HashSet<String> {
    let output = tokio::process::Command::new("gio")
        .args(["mount", "--list"])
        .output()
        .await;
    let mut mounted = HashSet::new();
    if let Ok(out) = output {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(pos) = line.find("dav://") {
                let uri = line[pos..].split_whitespace().next().unwrap_or("").to_string();
                if !uri.is_empty() { mounted.insert(uri); }
            }
        }
    }
    mounted
}

async fn mount_share(uri: &str) -> bool {
    tokio::process::Command::new("gio")
        .args(["mount", uri])
        .output().await
        .map(|o| o.status.success()).unwrap_or(false)
}

async fn unmount_share(uri: &str) -> bool {
    tokio::process::Command::new("gio")
        .args(["mount", "--unmount", uri])
        .output().await
        .map(|o| o.status.success()).unwrap_or(false)
}
