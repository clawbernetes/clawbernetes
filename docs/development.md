# Development

How to build, test, and contribute to Clawbernetes.

---

## Prerequisites

- **Rust 1.85+** (2024 Edition)
- **Node.js 20+** (for the OpenClaw plugin)
- **Docker** (optional, for containerized testing)
- GPU drivers (optional, for hardware acceleration)

---

## Building

```bash
make build            # release build (all binaries)
make build-gateway    # gateway only
make build-node       # node only
make build-cli        # CLI only
```

---

## Testing

```bash
make test             # all tests (4,600+)
make test-fast        # unit tests only
make clippy           # lint
make check            # fmt + clippy + test
```

---

## Plugin Development

```bash
cd plugin/openclaw-clawbernetes
npm run dev           # watch mode (recompile on save)
npm run typecheck     # type check without emit
npm run build         # production build
```

---

## Project Stats

- 160,000+ lines of Rust
- 37 crates across 6 domains
- 4,600+ tests
- 0 `unsafe` in core libraries

---

## Code Standards

- No `unwrap()`/`expect()` in library code
- Tests required for all new functionality
- `cargo clippy -- -D warnings` must pass
- `cargo fmt` enforced

---

## See Also

- [Architecture](architecture.md) — System design and crate map
- [Docker Deployment](docker.md) — Running with Docker
- [Contributing Guide](../CONTRIBUTING.md) — Contribution workflow
