run:
    cargo build && ./target/debug/cosmic-share-browser

build:
    cargo build --release

install:
    cargo build --release
    mkdir -p ~/.local/bin
    mkdir -p ~/.config/systemd/user
    rm -f ~/.local/bin/cosmic-share-browser
    rm -f ~/.local/bin/cosmic-share-daemon
    cp target/release/cosmic-share-browser ~/.local/bin/
    cp target/release/cosmic-share-daemon ~/.local/bin/
    mkdir -p ~/.local/share/applications
    cp cosmic-share-browser.desktop ~/.local/share/applications/
    systemctl --user daemon-reload

uninstall:
    systemctl --user disable --now cosmic-share-daemon || true
    rm -f ~/.local/bin/cosmic-share-browser
    rm -f ~/.local/bin/cosmic-share-daemon
    rm -f ~/.config/systemd/user/cosmic-share-daemon.service
    rm -f ~/.local/share/applications/cosmic-share-browser.desktop
    rm -rf ~/.config/cosmic-share-browser
    systemctl --user daemon-reload

clean:
    cargo clean
