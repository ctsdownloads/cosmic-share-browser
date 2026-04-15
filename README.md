# cosmic-share-browser

A COSMIC desktop panel applet for sharing files over WebDAV and browsing network shares via Avahi/mDNS.

![screenshot](screenshot.png)

## What it does

**Share files** — Shares a directory (default `~/Public`) as a WebDAV server on your local network, advertised via Avahi for automatic discovery. Read-only by default, read-write toggle available.

**Browse shares** — Discovers WebDAV shares on the LAN via mDNS. Mount and unmount with one click using `gio`.

The applet sits in your COSMIC panel. A background daemon (`cosmic-share-daemon`) runs the WebDAV server as a systemd user service.

## Features

- Read-only by default, read-write toggle in the applet
- Ephemeral port assignment (OS picks a free port each start)
- Automatic firewall management (UFW, firewalld, nftables, iptables)
- Stale firewall rule cleanup across reboots (single password prompt)
- Local share filtering (your own machine is hidden from the browse list)
- Avahi service advertisement (`_webdav._tcp`)
- Config hot-reload (daemon polls every 3 seconds)

## Dependencies

- [COSMIC desktop](https://github.com/pop-os/cosmic-epoch) with panel
- [Avahi](https://avahi.org/) — `avahi-daemon`, `avahi-browse`, `avahi-publish-service`
- `gio` (GLib/GVFS) — for mounting discovered shares
- A firewall (optional) — UFW, firewalld, nftables, or iptables

### Install dependencies

If you're already running COSMIC, most build deps are there. You mainly need Avahi, GVFS, Rust, and just.

**Arch / CachyOS:**

```sh
sudo pacman -S avahi nss-mdns gvfs rust just
sudo systemctl enable --now avahi-daemon
```

Make sure `/etc/nsswitch.conf` has `mdns_minimal [NOTFOUND=return]` in the `hosts` line for `.local` resolution. See [ArchWiki/Avahi](https://wiki.archlinux.org/title/Avahi) for details.

**Fedora:**

```sh
sudo dnf install avahi avahi-tools gvfs rust cargo just wayland-devel libxkbcommon-devel openssl-devel
sudo systemctl enable --now avahi-daemon
```

Fedora uses systemd-resolved for `.local` hostname resolution, so `nss-mdns` is not needed.

**Ubuntu / Pop!_OS:**

```sh
sudo apt install avahi-daemon avahi-utils gvfs gvfs-backends build-essential pkg-config libwayland-dev libxkbcommon-dev libssl-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install just
```

> **Note:** If the build fails with a missing header error, install the corresponding `-dev` (Ubuntu) or `-devel` (Fedora) package. See the [cosmic-epoch README](https://github.com/pop-os/cosmic-epoch) for the full build dependency list.

## Install

```sh
git clone https://github.com/ctsdownloads/cosmic-share-browser.git
cd cosmic-share-browser
just install
```

Builds both binaries, copies them to `~/.local/bin/`, installs the `.desktop` file, and reloads systemd.

### Add the applet

Open Settings > Desktop > Panel, click **+** to add an applet, pick **Network Share Browser**, drag it to the right section of the panel.

### First run

Click the applet icon. It shows "Service not installed" — click **Install & Start Sharing Service**. That creates the systemd user service and starts the daemon.

## Usage

Click the panel icon to open the popup.

**Sharing:** Hit "Enable" to start sharing. The daemon picks a random port, writes it to a port file, and advertises via Avahi. Hit "Open Port" to open the firewall (one password prompt). Change the shared directory and hit "Save & Apply" to restart with the new path.

**Read-only / Read-write:** Defaults to read-only. Click the "Read-Only" button to switch to read-write — it turns red when write access is on. The daemon restarts on its own. In read-only mode, PUT, DELETE, MKCOL, MOVE, COPY, PROPPATCH, LOCK, and UNLOCK get a 405 Method Not Allowed.

**Browsing:** The applet scans for `_webdav._tcp` services on the network at startup. Hit "Mount" to mount a share via `gio mount`, "Unmount" to remove it, "Scan" to refresh.

## How it works

| Component | Binary | Role |
|---|---|---|
| Applet (GUI) | `cosmic-share-browser` | Panel icon + popup controls |
| Daemon | `cosmic-share-daemon` | WebDAV server + Avahi advertisement |
| Library | `cosmic_share_browser` | Shared config and firewall logic |

The daemon runs as `~/.config/systemd/user/cosmic-share-daemon.service`. It reads `~/.config/cosmic-share-browser/config.toml` and re-checks every 3 seconds for changes. When sharing is enabled it binds an ephemeral port, writes the port to `$XDG_RUNTIME_DIR/cosmic-share-daemon.port`, and starts the WebDAV server using `dav-server` + `hyper`.

The applet reads the port file for display and firewall management via `pkexec`. On startup it cleans up stale firewall rules from the previous session and opens the new port in one password prompt.

## Configuration

`~/.config/cosmic-share-browser/config.toml`:

```toml
enabled = true
shared_dir = "/home/user/Public"
service_name = "COSMIC-Share"
read_only = true
```

## Security

- **No authentication.** Anyone on the LAN can access the shared directory. Same model as macOS Public folder sharing or Samba guest access.
- **Read-only by default.** Write access requires an explicit toggle.
- **Write-method blocking** happens at the HTTP layer before the request hits the WebDAV handler.
- **Firewall rules** are ephemeral — they clean up on reboot. The applet tracks the last opened port and removes stale rules on startup.

## Uninstall

```sh
just uninstall
```

Then remove the applet from the panel in Settings > Desktop > Panel.

## License

GPL-3.0
