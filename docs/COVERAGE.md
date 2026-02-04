# Test Coverage Report

*Generated: 2026-02-03*

## Summary

| Metric | Value |
|--------|-------|
| **Total Tests** | 2,005+ |
| **Unit Tests** | 1,917 |
| **Integration Tests** | 88 |
| **Total Lines of Code** | 61,943 |
| **Tests per KLOC** | 30.9 |
| **Benchmark Suites** | 8 |

## Coverage by Crate

### Core Infrastructure

| Crate | Tests | Lines | Tests/KLOC | Grade |
|-------|------:|------:|----------:|:-----:|
| `claw-proto` | 88 | 2,234 | 39.3 | 🟢 |
| `claw-cli` | 117 | 3,027 | 38.6 | 🟢 |
| `claw-gateway` | 67 | 1,985 | 33.7 | 🟢 |
| `claw-gateway-server` | 67 | 2,101 | 31.8 | 🟢 |
| `clawnode` | 159 | 6,377 | 24.9 | 🟡 |

### Compute

| Crate | Tests | Lines | Tests/KLOC | Grade |
|-------|------:|------:|----------:|:-----:|
| `claw-compute` | 26 | 1,647 | 15.7 | 🟡 |

*Note: GPU tests require hardware; additional coverage via manual testing on Metal/CUDA.*

### Operations & Observability

| Crate | Tests | Lines | Tests/KLOC | Grade |
|-------|------:|------:|----------:|:-----:|
| `claw-metrics` | 117 | 3,116 | 37.5 | 🟢 |
| `claw-logs` | 72 | 2,565 | 28.0 | 🟡 |
| `claw-observe` | 140 | 4,640 | 30.1 | 🟢 |
| `claw-deploy` | 115 | 3,239 | 35.5 | 🟢 |
| `claw-rollback` | 173 | 4,880 | 35.4 | 🟢 |

### Security

| Crate | Tests | Lines | Tests/KLOC | Grade |
|-------|------:|------:|----------:|:-----:|
| `claw-secrets` | 100 | 2,764 | 36.1 | 🟢 |
| `claw-pki` | 71 | 2,468 | 28.7 | 🟡 |

### Networking

| Crate | Tests | Lines | Tests/KLOC | Grade |
|-------|------:|------:|----------:|:-----:|
| `claw-network` | 70 | 2,794 | 25.0 | 🟡 |
| `claw-wireguard` | 35 | 1,808 | 19.3 | 🟡 |
| `claw-tailscale` | 74 | 2,107 | 35.1 | 🟢 |

### MOLT Marketplace

| Crate | Tests | Lines | Tests/KLOC | Grade |
|-------|------:|------:|----------:|:-----:|
| `molt-core` | 48 | 1,018 | 47.1 | 🟢 |
| `molt-token` | 48 | 2,188 | 21.9 | 🟡 |
| `molt-market` | 27 | 1,213 | 22.2 | 🟡 |
| `molt-agent` | 103 | 2,404 | 42.8 | 🟢 |
| `molt-p2p` | 170 | 5,950 | 28.5 | 🟡 |
| `molt-attestation` | 30 | 1,418 | 21.1 | 🟡 |

## Grade Legend

| Grade | Tests/KLOC | Meaning |
|:-----:|:----------:|---------|
| 🟢 | ≥30 | Good coverage |
| 🟡 | 15-30 | Moderate coverage |
| 🔴 | <15 | Needs improvement |

## Integration Tests

| Test Suite | Tests | Description |
|------------|------:|-------------|
| `e2e_flow_test` | 23 | End-to-end node lifecycle |
| `gateway_integration` | 12 | Gateway-node communication |
| `node_integration` | 5 | Node registration & heartbeat |
| `dispatch_test` | 7 | Workload scheduling |
| `attestation_flow_test` | 12 | Hardware attestation |
| `molt_flow_test` | 15 | Marketplace transactions |
| `negotiation_test` | 35 | Price negotiation |

## Benchmarks

| Suite | Crate | Focus |
|-------|-------|-------|
| `market_benchmarks` | molt-market | Orderbook operations |
| `node_benchmarks` | clawnode | Message handling |
| `molt_core_benchmarks` | molt-core | Token arithmetic |
| `metrics_benchmarks` | claw-metrics | TSDB performance |
| `logs_benchmarks` | claw-logs | Log ingestion |
| `p2p_benchmarks` | molt-p2p | Gossip protocol |
| `agent_benchmarks` | molt-agent | Decision making |
| `attestation_benchmarks` | molt-attestation | Verification |

## Test Categories

### By Type

| Type | Count | Percentage |
|------|------:|----------:|
| Unit Tests | 1,917 | 95.6% |
| Integration Tests | 88 | 4.4% |

### By Domain

| Domain | Tests | Percentage |
|--------|------:|----------:|
| Core Infrastructure | 498 | 24.8% |
| Operations | 617 | 30.8% |
| Security | 171 | 8.5% |
| Networking | 179 | 8.9% |
| MOLT Marketplace | 426 | 21.2% |
| Compute | 26 | 1.3% |
| Integration | 88 | 4.4% |

## Running Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p clawnode

# With GPU features
cargo test -p claw-compute --features cubecl-wgpu

# Integration tests only
cargo test -p molt-integration-tests

# With output
cargo test --workspace -- --nocapture

# Benchmarks
cargo bench --workspace
```

## Coverage Tools

### Using cargo-tarpaulin (Linux)

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --workspace --out Html
# Open tarpaulin-report.html
```

### Using cargo-llvm-cov (All platforms)

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --html
# Open target/llvm-cov/html/index.html
```

## Improvement Areas

### Priority 1 (Critical Paths)

- [ ] `claw-compute` — Add more GPU kernel tests
- [ ] `molt-market` — Increase settlement coverage
- [ ] `molt-attestation` — More verification scenarios

### Priority 2 (Security)

- [ ] `claw-wireguard` — Key rotation tests
- [ ] `claw-pki` — Certificate chain validation

### Priority 3 (Edge Cases)

- [ ] `clawnode` — Network partition handling
- [ ] `molt-p2p` — Gossip convergence tests

## CI Integration

Tests run automatically on:
- Every push to `main`
- Every pull request
- Nightly for extended tests

See `.github/workflows/ci.yml` for configuration.

---

*Coverage is measured in tests per 1000 lines of code (Tests/KLOC). Industry average for well-tested Rust projects is 20-40 Tests/KLOC.*
