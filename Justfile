# Abort on first error, treat unset vars as errors, fail pipes loudly
set shell := ["bash", "-euo", "pipefail", "-c"]

run:
    cargo build && ./target/debug/cosmic-share-browser

build:
    cargo build --release
    test -s target/release/cosmic-share-browser
    test -s target/release/cosmic-share-daemon

install: build
    mkdir -p ~/.local/bin
    mkdir -p ~/.local/share/applications
    install -Dm755 target/release/cosmic-share-browser ~/.local/bin/cosmic-share-browser
    install -Dm755 target/release/cosmic-share-daemon  ~/.local/bin/cosmic-share-daemon
    install -Dm644 cosmic-share-browser.desktop ~/.local/share/applications/cosmic-share-browser.desktop
    systemctl --user daemon-reload
    @echo ""
    @echo "Installed:"
    @ls -la ~/.local/bin/cosmic-share-browser ~/.local/bin/cosmic-share-daemon
    @echo ""
    @echo "Next: click the applet and 'Install & Start Sharing Service'."

uninstall:
    -systemctl --user disable --now cosmic-share-daemon
    rm -f ~/.local/bin/cosmic-share-browser
    rm -f ~/.local/bin/cosmic-share-daemon
    rm -f ~/.config/systemd/user/cosmic-share-daemon.service
    rm -f ~/.local/share/applications/cosmic-share-browser.desktop
    rm -rf ~/.config/cosmic-share-browser
    systemctl --user daemon-reload

clean:
    cargo clean
