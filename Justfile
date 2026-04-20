# Justfile for github.com/ctsdownloads/cosmic-share-browser
# Two-binary install: applet + daemon (both go to ~/.local/bin)
#
# User-scope install only — the applet's systemd user service hardcodes
# ExecStart=%h/.local/bin/cosmic-share-daemon (see src/main.rs line 16),
# so a /usr install would generate a broken unit. `install` is aliased
# to `install-user` for naming consistency with sibling ctsdownloads
# COSMIC repos; both do the same user-local install.

# Abort on first error, treat unset vars as errors, fail pipes loudly
set shell := ["bash", "-euo", "pipefail", "-c"]

alias install := install-user
alias uninstall := uninstall-user

name := 'cosmic-share-browser'
daemon := 'cosmic-share-daemon'
desktop := 'cosmic-share-browser.desktop'

user-prefix := env_var('HOME') + '/.local'
user-bin-dst := user-prefix + '/bin/' + name
user-daemon-dst := user-prefix + '/bin/' + daemon
user-desktop-dst := user-prefix + '/share/applications/' + desktop

default: build-release

# Run a debug build from source for testing
run:
    cargo build && ./target/debug/{{name}}

# Build optimized release binaries + fail loudly if either is missing/empty
build-release:
    cargo build --release
    test -s target/release/{{name}}
    test -s target/release/{{daemon}}

# Build and install user-local (no sudo; works on Fedora/Atomic/Pop/CachyOS/NixOS)
# If run inside a toolbox/distrobox, the systemctl daemon-reload step is
# a no-op — run `systemctl --user daemon-reload` on the host after exit.
install-user: build-release
    install -Dm755 target/release/{{name}} {{user-bin-dst}}
    install -Dm755 target/release/{{daemon}} {{user-daemon-dst}}
    install -Dm644 {{desktop}} {{user-desktop-dst}}
    sed -i 's|^Exec=.*|Exec={{user-bin-dst}}|' {{user-desktop-dst}}
    -systemctl --user daemon-reload
    @echo ""
    @echo "Installed:"
    @ls -la {{user-bin-dst}} {{user-daemon-dst}}
    @echo ""
    @echo "Next: click the applet and 'Install & Start Sharing Service'."

# Remove user-local install
uninstall-user:
    -systemctl --user disable --now {{daemon}}
    rm -f {{user-bin-dst}}
    rm -f {{user-daemon-dst}}
    rm -f ~/.config/systemd/user/{{daemon}}.service
    rm -f {{user-desktop-dst}}
    rm -rf ~/.config/cosmic-share-browser
    -systemctl --user daemon-reload

clean:
    cargo clean
