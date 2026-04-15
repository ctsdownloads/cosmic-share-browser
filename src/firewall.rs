#[derive(Debug, Clone, PartialEq)]
pub enum Firewall {
    Ufw,
    Firewalld,
    Nftables,
    Iptables,
    None,
}

impl Firewall {
    pub fn detect() -> Self {
        if service_active("ufw") { return Self::Ufw; }
        if service_active("firewalld") { return Self::Firewalld; }
        if service_active("nftables") { return Self::Nftables; }
        if cmd_exists("iptables") { return Self::Iptables; }
        Self::None
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Ufw => "UFW",
            Self::Firewalld => "firewalld",
            Self::Nftables => "nftables",
            Self::Iptables => "iptables",
            Self::None => "none detected",
        }
    }

    /// Command args to allow a port.  Does NOT include pkexec — caller decides
    /// whether to prepend it.
    ///
    /// firewalld: runtime-only rule (no --permanent).  The port is ephemeral so
    /// the rule should be too — it auto-cleans on reboot / firewalld restart.
    pub fn allow_args(&self, port: u16) -> Vec<String> {
        match self {
            Self::Ufw => vec![
                "ufw".into(), "allow".into(), format!("{}/tcp", port),
            ],
            Self::Firewalld => vec![
                "firewall-cmd".into(),
                format!("--add-port={}/tcp", port),
            ],
            Self::Nftables => vec![
                "nft".into(), "add".into(), "rule".into(),
                "inet".into(), "filter".into(), "input".into(),
                "tcp".into(), "dport".into(), port.to_string(),
                "accept".into(),
            ],
            Self::Iptables => vec![
                "iptables".into(), "-A".into(), "INPUT".into(),
                "-p".into(), "tcp".into(),
                "--dport".into(), port.to_string(),
                "-j".into(), "ACCEPT".into(),
            ],
            Self::None => vec![],
        }
    }

    /// Command args to deny (remove) a port rule.
    ///
    /// nftables: deletes by handle — requires a shell pipeline to look up the
    /// handle first.  Returns a sh -c wrapper.
    pub fn deny_args(&self, port: u16) -> Vec<String> {
        match self {
            Self::Ufw => vec![
                "ufw".into(), "delete".into(), "allow".into(),
                format!("{}/tcp", port),
            ],
            Self::Firewalld => vec![
                "firewall-cmd".into(),
                format!("--remove-port={}/tcp", port),
            ],
            Self::Nftables => {
                // nft requires a rule handle to delete.  Look it up first.
                let script = format!(
                    "handle=$(nft -a list chain inet filter input 2>/dev/null \
                     | grep 'tcp dport {} accept' \
                     | grep -oP 'handle \\K\\d+' | head -1); \
                     [ -n \"$handle\" ] && nft delete rule inet filter input handle $handle",
                    port
                );
                vec!["sh".into(), "-c".into(), script]
            }
            Self::Iptables => vec![
                "iptables".into(), "-D".into(), "INPUT".into(),
                "-p".into(), "tcp".into(),
                "--dport".into(), port.to_string(),
                "-j".into(), "ACCEPT".into(),
            ],
            Self::None => vec![],
        }
    }
}

fn service_active(name: &str) -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn cmd_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Direct (no pkexec) — used by the daemon.
//
// firewalld checks polkit internally, so `firewall-cmd --add-port=…` works
// when the caller's session has an active polkit agent.  From a systemd --user
// service this usually succeeds on Fedora desktops.  For ufw/iptables/nftables
// it will fail without root — that's fine, the GUI can retry with pkexec.
// ---------------------------------------------------------------------------

/// Try to open a firewall port without privilege escalation.
/// Returns true on success or if no firewall is active.
pub async fn allow_port_direct(port: u16) -> bool {
    let fw = Firewall::detect();
    if fw == Firewall::None { return true; }
    let args = fw.allow_args(port);
    if args.is_empty() { return true; }
    tokio::process::Command::new(&args[0])
        .args(&args[1..])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Try to close a firewall port without privilege escalation.
pub async fn deny_port_direct(port: u16) -> bool {
    let fw = Firewall::detect();
    if fw == Firewall::None { return true; }
    let args = fw.deny_args(port);
    if args.is_empty() { return true; }
    tokio::process::Command::new(&args[0])
        .args(&args[1..])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// pkexec-wrapped — used by the GUI (has polkit agent on-screen).
// ---------------------------------------------------------------------------

/// Open a firewall port, using pkexec for privilege escalation.
pub async fn allow_port(port: u16) -> bool {
    let fw = Firewall::detect();
    if fw == Firewall::None { return true; }
    let args = fw.allow_args(port);
    if args.is_empty() { return true; }
    tokio::process::Command::new("pkexec")
        .args(&args)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Close a firewall port, using pkexec for privilege escalation.
pub async fn deny_port(port: u16) -> bool {
    let fw = Firewall::detect();
    if fw == Firewall::None { return true; }
    let args = fw.deny_args(port);
    if args.is_empty() { return true; }
    tokio::process::Command::new("pkexec")
        .args(&args)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Close old port and open new port in a single pkexec call.
/// If old_port is None, just opens the new port.
pub async fn swap_port(old_port: Option<u16>, new_port: u16) -> bool {
    let fw = Firewall::detect();
    if fw == Firewall::None { return true; }

    let mut script = String::new();

    // Build deny command for old port
    if let Some(old) = old_port {
        if old != new_port {
            let deny = fw.deny_args(old);
            if !deny.is_empty() {
                script.push_str(&shell_escape_args(&deny));
                script.push_str("; ");
            }
        }
    }

    // Build allow command for new port
    let allow = fw.allow_args(new_port);
    if allow.is_empty() { return true; }
    script.push_str(&shell_escape_args(&allow));

    tokio::process::Command::new("pkexec")
        .args(["sh", "-c", &script])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn shell_escape_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.contains(' ') || a.contains(';') || a.contains('&')
                || a.contains('|') || a.contains('$')
            {
                format!("'{}'", a.replace('\'', "'\\''"))
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
