.PHONY: build test check release clean plugin

# Build all crates (debug)
build:
	cargo build --workspace

# Build release binaries
release:
	cargo build --workspace --release

# Build clawnode with all features
clawnode-full:
	cargo build -p clawnode --features full --release

# Run all tests
test:
	cargo test --workspace

# Type-check without building
check:
	cargo check --workspace

# Lint
lint:
	cargo clippy --workspace -- -D warnings

# Format
fmt:
	cargo fmt --all

# Build the OpenClaw plugin
plugin:
	cd plugin/openclaw-clawbernetes && npm install && npx tsc

# Install clawnode locally
install:
	cargo install --path crates/clawnode

# Install with all features
install-full:
	cargo install --path crates/clawnode --features full

# Clean build artifacts
clean:
	cargo clean
	rm -rf plugin/openclaw-clawbernetes/dist
