# Default recipe - show available commands
default:
    @just --list

# Build the embedded accounts library
build:
    cargo build --release -p accounts

# Build only the library (kept for compatibility with older scripts)
build-lib:
    cargo build --release -p accounts

# Run all tests
test:
    cargo test --workspace --all-features

# Check headless and feature-gated builds
check-features:
    cargo check -p accounts --no-default-features
    cargo check -p accounts --no-default-features --features calendar
    cargo check -p accounts --no-default-features --features mail
    cargo check -p accounts --no-default-features --features contacts
    cargo check -p accounts --no-default-features --features tasks
    cargo check -p accounts --no-default-features --features calendar,mail,contacts,tasks,libcosmic

# Run clippy linting
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Format code
format:
    cargo fmt

# Check formatting
check-format:
    cargo fmt --check

# Run all checks (test, feature builds, lint, format)
check: test check-features lint check-format

# Clean build artifacts
clean:
    cargo clean

# Generate documentation
docs:
    cargo doc -p accounts --all-features --no-deps --open

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

# Full workspace build check
workspace-check:
    cargo check --workspace --all-targets --all-features

# Development: watch for changes and run tests
dev-watch:
    cargo watch -s "just test"

# Help for setting up the development environment
dev-setup:
    @echo "Development setup:"
    @echo "1. Install Rust toolchain: https://rustup.rs/"
    @echo "2. Install development tools:"
    @echo "   cargo install cargo-watch cargo-audit cargo-outdated"
    @echo "3. Run 'just check' to verify everything works"
