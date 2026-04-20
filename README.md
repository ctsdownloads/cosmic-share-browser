# cosmic-share-browser

A COSMIC desktop panel applet for sharing files over WebDAV and browsing network shares via Avahi/mDNS.

![cosmic-share-browser applet](screenshot.png)

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

- COSMIC desktop with panel
- **Avahi** — `avahi-daemon`, `avahi-browse`, `avahi-publish-service`
- **gio** (GLib/GVFS) — for mounting discovered shares
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
sudo dnf install avahi avahi-tools gvfs \
  gcc gcc-c++ cmake pkgconf-pkg-config just git \
  rust cargo clang-devel \
  systemd-devel openssl-devel \
  wayland-devel libxkbcommon-devel mesa-libEGL-devel \
  libinput-devel libseat-devel \
  expat-devel fontconfig-devel freetype-devel
sudo systemctl enable --now avahi-daemon
```

Fedora uses `systemd-resolved` for `.local` hostname resolution, so `nss-mdns` is not needed.

**Fedora Atomic (COSMIC spin) / Origami:**

On immutable Fedora variants, `/usr` is read-only. The daemon's systemd user unit hardcodes `ExecStart=%h/.local/bin/cosmic-share-daemon`, so this applet is user-local only by design — `just install` is aliased to `install-user` and both write to `~/.local`. Build inside a Fedora distrobox. Full tested procedure:

1. Pre-flight host checks — avahi, gio, pkexec, firewalld:

   ```sh
   systemctl status avahi-daemon | head -3
   command -v gio
   command -v pkexec
   sudo firewall-cmd --state
   ```

   Expected: avahi-daemon active/enabled, `gio` at `/usr/bin/gio`, `pkexec` at `/usr/bin/pkexec`, firewalld reports `running`. If avahi-daemon is inactive:

   ```sh
   sudo systemctl enable --now avahi-daemon
   ```

2. Create and enter a Fedora 43 distrobox (reuse `cosmic-build` if it still exists):

   ```sh
   distrobox create --name cosmic-build --image registry.fedoraproject.org/fedora-toolbox:43
   distrobox enter cosmic-build
   ```

3. Inside the container, install build deps:

   ```sh
   sudo dnf install -y gcc gcc-c++ cmake pkgconf-pkg-config just git \
     rust cargo clang-devel \
     systemd-devel openssl-devel \
     wayland-devel libxkbcommon-devel mesa-libEGL-devel \
     libinput-devel libseat-devel \
     expat-devel fontconfig-devel freetype-devel
   ```

4. Clone:

   ```sh
   cd ~
   git clone https://github.com/ctsdownloads/cosmic-share-browser.git
   cd cosmic-share-browser
   ```

5. Build and install user-local:

   ```sh
   just install-user
   ```

   The `systemctl --user daemon-reload` at the end is a no-op inside distrobox — expected, we reload on the host in step 8.

6. Verify both binaries landed and `Exec=` is absolute:

   ```sh
   file ~/.local/bin/cosmic-share-browser ~/.local/bin/cosmic-share-daemon
   grep ^Exec= ~/.local/share/applications/cosmic-share-browser.desktop
   ```

   Both `file` outputs should say `ELF 64-bit LSB pie executable`. `grep` should show `Exec=/home/$USER/.local/bin/cosmic-share-browser`.

7. Exit the container:

   ```sh
   exit
   ```

8. On the host, reload systemd user manager:

   ```sh
   systemctl --user daemon-reload
   ```

9. Restart the COSMIC panel:

   ```sh
   killall cosmic-panel
   ```

10. COSMIC: **Settings → Desktop → Panel → Configure Panel Applets → + Add Applet → drag Network Share Browser** in.

11. Click the applet icon in the panel. It'll show "Service not installed" — click **Install & Start Sharing Service**. That writes `~/.config/systemd/user/cosmic-share-daemon.service` and starts the daemon.

12. Verify the daemon is running:

    ```sh
    systemctl --user status cosmic-share-daemon
    ls -la $XDG_RUNTIME_DIR/cosmic-share-daemon.port
    ```

    Service should be `active (running)`, port file should exist.

13. Enable sharing from the applet popup (default share dir is `~/Public`). Click **Open Port** when you want the firewall rule. Expect one or two polkit prompts on firewalld — firewalld runs its own auth checks on top of pkexec. Rules are ephemeral (cleaned up on reboot, re-opened on each sharing enable); no pre-configured permanent firewall rule needed.

14. Verify the port is open while sharing is active:

    ```sh
    cat $XDG_RUNTIME_DIR/cosmic-share-daemon.port
    sudo firewall-cmd --list-ports
    ```

    The number in the port file should match one of the entries in `--list-ports`.

Uninstall later: from the cloned repo on the host, `just uninstall-user`, then remove the applet from the panel in Settings.

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

Builds both binaries, verifies they're non-empty, copies them to `~/.local/bin/`, installs the `.desktop` file, and reloads systemd.

Verify the install succeeded:

```sh
file ~/.local/bin/cosmic-share-browser ~/.local/bin/cosmic-share-daemon
```

Expected: both show `ELF 64-bit LSB pie executable`. If either shows `empty` or is missing, see Troubleshooting below.

### Add the applet

Open Settings > Desktop > Panel, click **+** to add an applet, pick **Network Share Browser**, drag it to the right section of the panel.

### First run

Click the applet icon. It shows "Service not installed" — click **Install & Start Sharing Service**. That creates the systemd user service and starts the daemon.

## Usage

Click the panel icon to open the popup.

**Sharing:** Hit **Enable** to start sharing. The daemon picks a random port, writes it to a port file, and advertises via Avahi. Hit **Open Port** to open the firewall. Change the shared directory and hit **Save & Apply** to restart with the new path.

**Read-only / Read-write:** Defaults to read-only. Click the "Read-Only" button to switch to read-write — it turns red when write access is on. The daemon restarts on its own. In read-only mode, PUT, DELETE, MKCOL, MOVE, COPY, PROPPATCH, LOCK, and UNLOCK get a 405 Method Not Allowed.

**Browsing:** The applet scans for `_webdav._tcp` services on the network at startup. Hit **Mount** to mount a share via `gio mount`, **Unmount** to remove it, **Scan** to refresh.

## How it works

| Component | Binary | Role |
|---|---|---|
| Applet (GUI) | `cosmic-share-browser` | Panel icon + popup controls |
| Daemon | `cosmic-share-daemon` | WebDAV server + Avahi advertisement |
| Library | `cosmic_share_browser` | Shared config and firewall logic |

The daemon runs as `~/.config/systemd/user/cosmic-share-daemon.service`. It reads `~/.config/cosmic-share-browser/config.toml` and re-checks every 3 seconds for changes. When sharing is enabled it binds an ephemeral port, writes the port to `$XDG_RUNTIME_DIR/cosmic-share-daemon.port`, and starts the WebDAV server using `dav-server` + `hyper`.

The applet reads the port file for display and manages the firewall via `pkexec`. On startup it cleans up stale firewall rules from the previous session and opens the new port. Depending on your firewall backend, this may trigger one or more authentication prompts (firewalld, for example, runs its own polkit checks on top of `pkexec`).

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
- Write-method blocking happens at the HTTP layer before the request hits the WebDAV handler.
- Firewall rules are ephemeral — they clean up on reboot. The applet tracks the last opened port and removes stale rules on startup.

## Troubleshooting

**Daemon fails with `status=203/EXEC` / `Exec format error`**

The installed binary is missing, empty, or wrong architecture. Verify:

```sh
file ~/.local/bin/cosmic-share-daemon
```

Expected: `ELF 64-bit LSB pie executable`. If it shows `empty` or the file is 0 bytes, the build failed silently during install. Clean and rebuild:

```sh
just clean
just install
```

Watch for `cargo` errors in the output.

**Build fails with a missing header error**

Install the dev packages listed under Dependencies. On Fedora you typically need `wayland-devel`, `libxkbcommon-devel`, and `openssl-devel`; on Ubuntu, `libwayland-dev`, `libxkbcommon-dev`, and `libssl-dev`.

**Port file missing / applet shows "Port pending"**

The daemon either isn't running or failed to bind. Check:

```sh
systemctl --user status cosmic-share-daemon
ls -la $XDG_RUNTIME_DIR/cosmic-share-daemon.port
```

If the service is in `activating (auto-restart)` with exit code 203, see the `Exec format error` section above.

## Uninstall

```sh
just uninstall
```

Then remove the applet from the panel in Settings > Desktop > Panel.

## License

GPL-3.0
