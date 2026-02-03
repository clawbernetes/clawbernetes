# Clawbernetes Makefile
# Common commands for development and deployment

.PHONY: all build test clean docker docker-up docker-down release help

# Default target
all: build test

# ===========================================
# Build
# ===========================================

build:
	cargo build --release

build-gateway:
	cargo build --release -p claw-gateway-server

build-node:
	cargo build --release -p clawnode

build-cli:
	cargo build --release -p claw-cli

# ===========================================
# Test
# ===========================================

test:
	cargo test --workspace

test-fast:
	cargo test --workspace --lib

clippy:
	cargo clippy --workspace -- -D warnings

fmt:
	cargo fmt --all

check: fmt clippy test

# ===========================================
# Docker
# ===========================================

docker:
	docker compose build

docker-up:
	docker compose up -d

docker-down:
	docker compose down

docker-logs:
	docker compose logs -f

docker-gateway:
	docker build --target gateway -t clawbernetes/gateway:latest .

docker-node:
	docker build --target node -t clawbernetes/node:latest .

docker-gpu:
	docker build -f Dockerfile.gpu -t clawbernetes/node-gpu:latest .

# ===========================================
# Run locally
# ===========================================

run-gateway:
	./target/release/claw-gateway

run-node:
	CLAWNODE_GATEWAY=ws://localhost:8080 ./target/release/clawnode --name local-node

status:
	./target/release/clawbernetes status

nodes:
	./target/release/clawbernetes node list

# ===========================================
# Release
# ===========================================

release: check
	cargo build --release
	@echo "Binaries in target/release/"
	@ls -lh target/release/claw-gateway target/release/clawnode target/release/clawbernetes

# ===========================================
# Clean
# ===========================================

clean:
	cargo clean

clean-docker:
	docker compose down -v --rmi local

# ===========================================
# Help
# ===========================================

help:
	@echo "Clawbernetes Build System"
	@echo ""
	@echo "Build:"
	@echo "  make build          - Build all binaries (release)"
	@echo "  make build-gateway  - Build gateway only"
	@echo "  make build-node     - Build node only"
	@echo "  make build-cli      - Build CLI only"
	@echo ""
	@echo "Test:"
	@echo "  make test           - Run all tests"
	@echo "  make test-fast      - Run unit tests only"
	@echo "  make clippy         - Run clippy lints"
	@echo "  make check          - fmt + clippy + test"
	@echo ""
	@echo "Docker:"
	@echo "  make docker         - Build Docker images"
	@echo "  make docker-up      - Start cluster (gateway + 2 nodes)"
	@echo "  make docker-down    - Stop cluster"
	@echo "  make docker-logs    - Follow logs"
	@echo "  make docker-gpu     - Build GPU node image"
	@echo ""
	@echo "Run:"
	@echo "  make run-gateway    - Run gateway locally"
	@echo "  make run-node       - Run node locally"
	@echo "  make status         - Check cluster status"
	@echo "  make nodes          - List nodes"
	@echo ""
	@echo "Release:"
	@echo "  make release        - Full release build"
	@echo "  make clean          - Clean build artifacts"
