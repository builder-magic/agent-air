# Default recipe
default:
    @just --list

# Build the crate
build:
    @cargo build

# Run tests
test:
    @cargo test

# Run clippy
lint:
    @cargo clippy -- -D warnings

# Format code
fmt:
    @cargo fmt

# Check formatting
fmt-check:
    @cargo fmt -- --check

# Clean build artifacts
clean:
    @cargo clean
