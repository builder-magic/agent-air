# Default recipe
default:
    @just --list

# Build the workspace
build:
    @cargo build --workspace

# Run tests
test:
    @cargo test --workspace

# Run clippy
lint:
    @cargo clippy --workspace -- -D warnings

# Format code
fmt:
    @cargo fmt --all

# Check formatting
fmt-check:
    @cargo fmt --all -- --check

# Clean build artifacts
clean:
    @cargo clean

# Generate documentation
doc:
    cargo doc --no-deps --workspace
