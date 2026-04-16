# cosmic-headphone-manager

A COSMIC desktop panel applet for managing Bluetooth and USB wireless headphones — battery level, codec display, profile switching, and auto-switch control.

![Bluetooth and USB headphones connected](screenshots/bluetooth-and-usb-connected.png)

## What it does

**Battery display** — Shows battery percentage with a visual bar for Bluetooth headphones (via upower) and USB wireless headsets (via [HeadsetControl](https://github.com/Sapd/HeadsetControl)).

**Codec display** — Shows the active Bluetooth codec (AAC, SBC, SBC-XQ, LDAC, aptX, etc.) extracted from the active profile.

**Profile switching** — One-click toggle between A2DP (stereo, high-quality) and HFP/HSP (voice, bidirectional for calls). Prefers MSBC over CVSD for voice quality.

**Auto-switch control** — Toggle WirePlumber's automatic profile switching on/off. When enabled, the system switches to voice mode when an app opens a mic stream (Zoom, Discord, Meet) and back to stereo when it closes.

**USB wireless headsets** — Detects SteelSeries, Logitech, Corsair, HyperX, and other USB wireless headsets via [HeadsetControl](https://github.com/Sapd/HeadsetControl). Shows battery level and headset name. The icon only appears when the headset is powered on, not just when the dongle is plugged in.

**Smart icon visibility** — The panel icon appears only when headphones are connected and hides when they're not. Uses `audio-headphones-symbolic` for Bluetooth and `audio-headset-symbolic` for USB wireless.

## Screenshots

| Bluetooth only | USB headset connected | Both connected | Dongle only (headset off) |
|---|---|---|---|
| ![BT](screenshots/bluetooth.png) | ![USB](screenshots/headset-connected.png) | ![Both](screenshots/bluetooth-and-usb-connected.png) | ![Dongle](screenshots/headset-disconnected-usb-dongle-only.png) |

## Features

- Battery percentage with visual bar (Bluetooth via upower, USB via HeadsetControl)
- Active Bluetooth codec display (AAC, SBC, SBC-XQ, LDAC, aptX, etc.)
- One-click A2DP ↔ HFP/HSP profile toggle (prefers MSBC over CVSD)
- WirePlumber auto-switch toggle (automatic voice/stereo switching for calls)
- USB wireless headset support via HeadsetControl (optional)
- Smart icon — appears on connect, hides on disconnect
- Different panel icons for Bluetooth vs USB headsets
- Auto-switch toggle hidden when only USB headsets are connected
- "No signal" indicator when USB dongle is present but headset is off
- No background daemon — applet runs inside the COSMIC panel process
- Full device scan only runs while the popup is open
- Lightweight background presence check every 10 seconds (icon visibility only)

## Dependencies

- [COSMIC desktop](https://github.com/pop-os/cosmic-epoch) with panel
- [PipeWire](https://pipewire.org/) + [WirePlumber](https://pipewire.pages.freedesktop.org/wireplumber/) — audio backend
- `wpctl` — WirePlumber CLI (comes with WirePlumber)
- `pactl` — PulseAudio CLI (comes with PipeWire)
- `upower` — battery level reporting for Bluetooth devices
- `bluetoothctl` — Bluetooth device enumeration (from `bluez-utils` on Arch, `bluez` on Fedora/Ubuntu — already installed if Bluetooth works)
- [HeadsetControl](https://github.com/Sapd/HeadsetControl) (optional) — USB wireless headset battery and detection

### Install dependencies

If you're already running COSMIC, PipeWire and WirePlumber are there. You mainly need Rust and just.

**Arch / CachyOS:**

```sh
sudo pacman -S wireplumber upower rust just
```

For USB wireless headset support:

```sh
paru -S headsetcontrol
```

**Fedora:**

```sh
sudo dnf install wireplumber upower rust cargo just wayland-devel libxkbcommon-devel
```

For USB wireless headset support:

```sh
sudo dnf install headsetcontrol
```

**Ubuntu / Pop!_OS:**

```sh
sudo apt install wireplumber pipewire upower build-essential pkg-config libwayland-dev libxkbcommon-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install just
```

For USB wireless headset support, build [HeadsetControl](https://github.com/Sapd/HeadsetControl) from source (no apt package available).

> **Note:** If the build fails with a missing header error, install the corresponding `-dev` (Ubuntu) or `-devel` (Fedora) package. See the [cosmic-epoch README](https://github.com/pop-os/cosmic-epoch) for the full build dependency list.

## Install

```sh
git clone https://github.com/ctsdownloads/cosmic-headphone-manager.git
cd cosmic-headphone-manager
just install
```

Builds the binary, verifies it's non-empty, copies it to `~/.local/bin/`, and installs the `.desktop` file. Log out and back in for the panel to pick up the new applet.

Verify the install succeeded:

```sh
file ~/.local/bin/cosmic-headphone-manager
```

Expected: `ELF 64-bit LSB pie executable`. If it shows `empty` or the file is missing, see Troubleshooting below.

### Add the applet

Open Settings > Desktop > Panel, click **+** to add an applet, pick **Headphone Manager**, drag it to the right section of the panel.

## Usage

The applet icon appears in your panel when headphones are connected and hides when they're not.

**Click the icon** to open the popup showing all connected headphones.

**Bluetooth headphones** show device name, active codec (AAC, SBC, etc.), battery percentage with a visual bar, and a profile toggle button. Click **Stereo** to switch to voice mode for calls, click **Voice** to switch back to stereo for music.

**USB wireless headsets** (SteelSeries, Logitech, Corsair, HyperX) show device name, battery percentage, and connection type. Requires [HeadsetControl](https://github.com/Sapd/HeadsetControl) to be installed. If the headset is powered off but the dongle is plugged in, the popup shows "No signal".

**Auto-Switch** toggles WirePlumber's automatic profile switching. When on, the system switches to HFP/HSP when an app opens a mic stream (video calls) and back to A2DP when the stream closes. This toggle only appears when Bluetooth headphones are connected.

## How it works

There is no background daemon. The applet runs entirely within the COSMIC panel process.

- **Background presence check** runs every 10 seconds — a lightweight `wpctl status` check to detect if headphones are connected. This controls icon visibility.
- **Full scan** runs when the popup is opened and every 5 seconds while it's open. This calls `wpctl`, `pactl`, `upower`, `bluetoothctl`, and optionally `headsetcontrol` to gather device state.
- **Profile switching** uses `pactl set-card-profile` with the card name from `wpctl inspect`.
- **Auto-switch** uses `wpctl settings --save bluetooth.autoswitch-to-headset-profile`.

When the popup is closed, only the lightweight presence check runs. Zero CPU impact during normal use.

### Detection methods

| Device type | Detection | Battery | Profile switching |
|---|---|---|---|
| Bluetooth headphones | `wpctl status` (bluez5 tag) | upower via `bluetoothctl` MAC matching | `pactl set-card-profile` |
| USB wireless headsets | [HeadsetControl](https://github.com/Sapd/HeadsetControl) JSON API | HeadsetControl | Not applicable |

## Troubleshooting

**Applet icon doesn't appear after install**

Log out and back in. COSMIC panel only picks up new applets on session start.

**Build fails with a missing header error**

Install the dev packages listed under Dependencies. On Fedora you typically need `wayland-devel` and `libxkbcommon-devel`; on Ubuntu, `libwayland-dev` and `libxkbcommon-dev`.

**`file ~/.local/bin/cosmic-headphone-manager` shows `empty` or 0 bytes**

The build failed silently during a previous install. Clean and rebuild:

```sh
just clean
just install
```

Watch for `cargo` errors in the output.

**No Bluetooth devices shown in popup**

Verify Bluetooth is working at the system level:

```sh
bluetoothctl devices Connected
wpctl status | grep bluez
```

If `bluetoothctl` lists the device but the applet doesn't, check that the device profile is set to `a2dp-sink` or a headset profile via `pactl list cards short`.

## Uninstall

```sh
just uninstall
```

Then remove the applet from the panel in Settings > Desktop > Panel.

## License

GPL-3.0