app_id := "dev.edfloreshz.Accounts"
ui := "crates/ui"

# Default recipe - show available commands
default:
    @just --list

# Build all components
build:
    cargo build --release

# Build only the library
build-lib:
    cargo build --release --lib

# Build the app, daemon, and consent prompt.
build-gui:
    cargo build --release -p accounts_ui

# Run all tests
test:
    dbus-run-session -- cargo test --workspace

# Run clippy linting
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Format code
format:
    cargo fmt

# Check formatting
check-format:
    cargo fmt --check

# Run all checks (test, lint, format)
check: test lint check-format

# Clean build artifacts
clean:
    cargo clean

# Build and install the shared application files.
install: build-gui system-files
    @if [ -n "${XDG_RUNTIME_DIR:-}" ] && [ -S "$XDG_RUNTIME_DIR/bus" ]; then systemctl --user daemon-reload; fi

# Remove the shared application files. Account data lives in the user profile.
uninstall: (system-files "" "remove")

[private]
system-files $destdir="" $operation="install":
    #!/bin/sh
    set -eu
    put() {
        if [ "$operation" = remove ]; then
            sudo rm -f "$3"
        elif [ -n "$destdir" ]; then
            install -D -m "$1" "$2" "$destdir$3"
        else
            sudo install -D -m "$1" "$2" "$3"
        fi
    }

    put 0755 target/release/accounts_ui /usr/bin/accounts_ui
    put 0644 {{ui}}/resources/app.desktop /usr/share/applications/{{app_id}}.desktop
    put 0644 {{ui}}/resources/app.metainfo.xml /usr/share/metainfo/{{app_id}}.metainfo.xml
    put 0644 {{ui}}/resources/icons/hicolor/scalable/apps/icon.svg /usr/share/icons/hicolor/scalable/apps/{{app_id}}.svg
    for service in {{app_id}} {{app_id}}.ConsentPrompt; do
        put 0644 "{{ui}}/data/$service.service" "/usr/share/dbus-1/services/$service.service"
    done
    put 0644 {{ui}}/data/cosmic-accounts.service /usr/lib/systemd/user/cosmic-accounts.service
    put 0644 {{ui}}/data/{{app_id}}.policy /usr/share/polkit-1/actions/{{app_id}}.policy
    for provider in google microsoft; do
        target="/usr/share/accounts/providers/$provider.toml"
        if [ "$operation" = install ] && [ -z "$destdir" ] && [ -e "$target" ]; then
            continue
        fi
        put 0644 "{{ui}}/data/providers/$provider.toml" "$target"
    done

# Start the background service (user session)
start-daemon:
    systemctl --user daemon-reload
    systemctl --user enable cosmic-accounts.service
    systemctl --user start cosmic-accounts.service

# Stop the background service (user session)
stop-daemon:
    systemctl --user stop cosmic-accounts.service
    systemctl --user disable cosmic-accounts.service

# Check background service status
status:
    systemctl --user status cosmic-accounts.service

# View background service logs
logs:
    journalctl --user -u cosmic-accounts.service -f

# Run the background D-Bus service in the foreground with debug logging,
# without opening a window. Run from the repo root so it picks up
# crates/ui/data/providers/*.toml.
run-daemon:
    ACCOUNTS_HEADLESS=1 RUST_LOG=debug cargo run -p accounts_ui

# Run the GUI in the foreground with debug logging.
run-ui:
    RUST_LOG=debug cargo run -p accounts_ui

# Run the reference ProviderHandler (fake device-code flow) on the session bus.
# Point a provider manifest's [handler] at
# dev.edfloreshz.Accounts.ProviderHandler.Example /dev/edfloreshz/Accounts/ProviderHandler.
run-example-handler:
    RUST_LOG=debug cargo run -p accounts_example_handler

# Run the headless service + the GUI together for a full local test session.
# Ctrl-C stops the whole stack. Requires a running D-Bus session bus and a
# secret-service provider (gnome-keyring/kwallet) for credential storage;
# OAuth flows additionally need real client credentials in
# crates/ui/data/providers/*.toml.
run-stack:
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'kill 0' EXIT
    echo "Starting background service..."
    ACCOUNTS_HEADLESS=1 RUST_LOG=debug cargo run -p accounts_ui &
    sleep 1
    echo "Starting UI..."
    RUST_LOG=debug cargo run -p accounts_ui

# List providers currently advertised by a running daemon over D-Bus
# (requires busctl; useful to confirm capability advertisement without the UI).
list-providers:
    busctl --user call {{app_id}} /dev/edfloreshz/Accounts/Manager {{app_id}}.Manager ListProviders

# Development: watch for changes and run tests
dev-watch:
    cargo watch -s "just test"

# Generate documentation
docs:
    cargo doc --no-deps --open

# Check dependencies for security advisories
audit:
    cargo audit

# Update dependencies
update:
    cargo update

# Check for outdated dependencies
outdated:
    cargo outdated

# Benchmark (if any benchmarks exist)
bench:
    cargo bench

# Development: full workspace build check
workspace-check:
    cargo check --workspace

# Prepare provider manifests for a distributable desktop OAuth client.
configure-providers $providers:
    #!/bin/sh
    set -eu
    : "${GOOGLE_CLIENT_ID:?Set GOOGLE_CLIENT_ID}"
    : "${GOOGLE_CLIENT_SECRET:?Set GOOGLE_CLIENT_SECRET}"
    : "${MICROSOFT_CLIENT_ID:?Set MICROSOFT_CLIENT_ID}"

    system_providers=/usr/share/accounts/providers
    providers=$(realpath -e -- "$providers")
    staged=
    if [ "$providers" = "$system_providers" ]; then
        staged=$(mktemp -d)
        trap 'rm -rf -- "$staged"' EXIT
        cp "$providers/google.toml" "$providers/microsoft.toml" "$staged/"
        providers=$staged
    fi

    escape() {
        printf '%s' "$1" | sed 's/[\\"]/\\&/g; s/[\\&|]/\\&/g'
    }

    sed -i \
        -e 's|[$]{GOOGLE_CLIENT_ID}|'"$(escape "$GOOGLE_CLIENT_ID")|g" \
        -e 's|[$]{GOOGLE_CLIENT_SECRET}|'"$(escape "$GOOGLE_CLIENT_SECRET")|g" \
        "$providers/google.toml"
    sed -i -e 's|[$]{MICROSOFT_CLIENT_ID}|'"$(escape "$MICROSOFT_CLIENT_ID")|g" "$providers/microsoft.toml"

    if [ -n "$staged" ]; then
        sudo install -m 0644 "$staged/google.toml" "$staged/microsoft.toml" "$system_providers/"
    fi

# Build a system package for the current Debian-based release.
package-native $output="target/native": build-gui
    #!/bin/sh
    set -eu
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    root="$work/package"
    just system-files "$root"
    just configure-providers "$root/usr/share/accounts/providers"

    version=$(cargo pkgid --offline -p accounts_ui)
    version=${version##*@}
    arch=$(dpkg --print-architecture)
    mkdir -p "$work/debian" "$root/DEBIAN" "$output"
    cat > "$work/debian/control" <<'EOF'
    Source: cosmic-ext-accounts
    Maintainer: COSMIC Utilities

    Package: cosmic-ext-accounts
    Architecture: any
    Description: Online accounts for COSMIC
    EOF
    shlibs=$(cd "$work" && dpkg-shlibdeps -O -e"$root/usr/bin/accounts_ui")
    cat > "$root/DEBIAN/control" <<EOF
    Package: cosmic-ext-accounts
    Version: $version
    Architecture: $arch
    Maintainer: COSMIC Utilities
    Depends: ${shlibs#shlibs:Depends=}, dbus, polkitd, gnome-keyring, ca-certificates, xdg-utils
    Description: Online accounts for COSMIC
     Set up provider accounts once and share them with participating COSMIC apps.
    EOF
    dpkg-deb --root-owner-group --build "$root" "$output/cosmic-ext-accounts_${version}_${arch}.deb"

package: package-native

# Help for setting up development environment
dev-setup:
    @echo "Development setup:"
    @echo "1. Install Rust toolchain: https://rustup.rs/"
    @echo "2. Install system dependencies:"
    @echo "   Ubuntu/Debian: sudo apt install libdbus-1-dev libsecret-1-dev"
    @echo "   Fedora: sudo dnf install dbus-devel libsecret-devel"
    @echo "   Arch: sudo pacman -S dbus libsecret"
    @echo "3. Install development tools:"
    @echo "   cargo install cargo-watch cargo-audit cargo-outdated"
    @echo "4. Run 'just check' to verify everything works"
