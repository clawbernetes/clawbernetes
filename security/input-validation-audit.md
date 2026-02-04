# Clawbernetes Input Validation & Parsing Security Audit

**Audit Date:** 2026-02-04  
**Auditor:** Security Engineering (AI-Assisted)  
**Scope:** Input validation, deserialization, parsing, and integer handling across all crates

---

## Executive Summary

This audit reviewed all input handling in the Clawbernetes project, focusing on deserialization, network input, file parsing, and integer handling. Overall, the codebase demonstrates **strong security practices** with:

- Comprehensive use of checked arithmetic for financial amounts
- Proper input validation with explicit size limits
- Good error handling on parsing failures
- Type-safe ID generation using UUIDs

However, several areas require attention:

| Severity | Count | Description |
|----------|-------|-------------|
| **Critical** | 0 | - |
| **High** | 1 | Missing WebSocket message size limits |
| **Medium** | 4 | Various DoS and validation gaps |
| **Low** | 5 | Minor improvements recommended |
| **Info** | 3 | Best practice observations |

---

## Findings

### HIGH-1: No Explicit Size Limits on WebSocket Messages

**Severity:** High  
**Location:** `crates/claw-gateway-server/src/session.rs:102-130`

**Description:**  
The `process_ws_message` function deserializes incoming WebSocket messages without explicit size validation. An attacker could send extremely large JSON payloads to cause memory exhaustion.

```rust
pub fn process_ws_message(ws_msg: &WsMessage) -> ServerResult<Option<NodeMessage>> {
    match ws_msg {
        WsMessage::Text(text) => {
            let node_msg: NodeMessage = serde_json::from_str(text)?;  // No size check
            Ok(Some(node_msg))
        }
        WsMessage::Binary(data) => {
            let node_msg: NodeMessage = serde_json::from_slice(data)?; // No size check
            Ok(Some(node_msg))
        }
        // ...
    }
}
```

**Impact:**  
- Denial of Service via memory exhaustion
- Potential process crash from OOM

**Recommendation:**
```rust
const MAX_MESSAGE_SIZE: usize = 1_048_576; // 1 MiB

pub fn process_ws_message(ws_msg: &WsMessage) -> ServerResult<Option<NodeMessage>> {
    match ws_msg {
        WsMessage::Text(text) => {
            if text.len() > MAX_MESSAGE_SIZE {
                return Err(ServerError::Protocol("message too large".into()));
            }
            let node_msg: NodeMessage = serde_json::from_str(text)?;
            Ok(Some(node_msg))
        }
        // Similar for Binary...
    }
}
```

---

### MEDIUM-1: P2P Message Deserialization Without Size Limits

**Severity:** Medium  
**Location:** `crates/molt-p2p/src/message.rs:174-178`

**Description:**  
The `from_bytes` method deserializes P2P messages without checking payload size first:

```rust
pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
    serde_json::from_slice(bytes)  // No size validation
}
```

**Impact:**  
- DoS via large message payloads in P2P network
- Memory pressure on nodes

**Recommendation:**
```rust
const MAX_P2P_MESSAGE_SIZE: usize = 65_536; // 64 KiB

pub fn from_bytes(bytes: &[u8]) -> Result<Self, P2pError> {
    if bytes.len() > MAX_P2P_MESSAGE_SIZE {
        return Err(P2pError::Protocol("P2P message too large".into()));
    }
    serde_json::from_slice(bytes)
        .map_err(|e| P2pError::Protocol(format!("invalid message: {e}")))
}
```

---

### MEDIUM-2: Gossip Wire Protocol Version Check Incomplete

**Severity:** Medium  
**Location:** `crates/molt-p2p/src/gossip/message.rs:229-244`

**Description:**  
The gossip wire protocol decoding checks version but doesn't validate the message type discriminator:

```rust
pub fn decode_wire(bytes: &[u8]) -> Result<Self, P2pError> {
    let wire_msg = WireGossipMessage::decode(bytes)?;

    if wire_msg.version > WIRE_VERSION {
        return Err(P2pError::Protocol(...));
    }

    // msg_type is not validated against known types
    serde_json::from_slice(&wire_msg.payload)?
}
```

**Impact:**  
- Unknown message types silently fail during JSON deserialization
- Missing explicit handling for protocol evolution

**Recommendation:**
```rust
pub fn decode_wire(bytes: &[u8]) -> Result<Self, P2pError> {
    let wire_msg = WireGossipMessage::decode(bytes)?;

    if wire_msg.version > WIRE_VERSION {
        return Err(P2pError::Protocol("unsupported version".into()));
    }

    // Validate known message types
    if !matches!(wire_msg.msg_type, 1..=6) {
        return Err(P2pError::Protocol(format!(
            "unknown message type: {}", wire_msg.msg_type
        )));
    }

    serde_json::from_slice(&wire_msg.payload)
        .map_err(|e| P2pError::Protocol(format!("invalid payload: {e}")))
}
```

---

### MEDIUM-3: Natural Language Parser Potential Integer Parsing Edge Cases

**Severity:** Medium  
**Location:** `crates/claw-deploy/src/parser.rs:181-190`

**Description:**  
The `parse_percentage` function handles decimal fractions but has edge cases:

```rust
fn parse_percentage(s: &str) -> Option<u8> {
    let s = s.trim_end_matches('%');
    if let Ok(n) = s.parse::<u8>() {
        if n <= 100 { return Some(n); }
    }
    if let Ok(f) = s.parse::<f64>() {
        if f > 0.0 && f <= 1.0 {
            return Some((f * 100.0) as u8);  // Truncation, not rounding
        }
    }
    None
}
```

**Impact:**  
- "0.999" becomes 99%, not 100% (truncation vs rounding)
- Edge cases like "0.0" reject when it should be 0%

**Recommendation:**
```rust
fn parse_percentage(s: &str) -> Option<u8> {
    let s = s.trim_end_matches('%');
    if let Ok(n) = s.parse::<u8>() {
        if n <= 100 { return Some(n); }
    }
    if let Ok(f) = s.parse::<f64>() {
        if f >= 0.0 && f <= 1.0 {
            return Some((f * 100.0).round() as u8);  // Use round()
        }
    }
    None
}
```

---

### MEDIUM-4: Workload Log Lines Array Unbounded

**Severity:** Medium  
**Location:** `crates/claw-proto/src/messages.rs:40-46`

**Description:**  
The `WorkloadLogs` message contains an unbounded `Vec<String>` for log lines:

```rust
WorkloadLogs {
    workload_id: WorkloadId,
    lines: Vec<String>,  // Unbounded
    is_stderr: bool,
}
```

**Impact:**  
- Malicious node could send millions of log lines
- Memory exhaustion on gateway

**Recommendation:**  
Add validation in the handler:
```rust
const MAX_LOG_LINES: usize = 1000;
const MAX_LINE_LENGTH: usize = 4096;

if lines.len() > MAX_LOG_LINES {
    return Err(ServerError::Protocol("too many log lines".into()));
}
for line in &lines {
    if line.len() > MAX_LINE_LENGTH {
        return Err(ServerError::Protocol("log line too long".into()));
    }
}
```

---

### LOW-1: Amount Type Uses Floating Point Conversion

**Severity:** Low  
**Location:** `crates/molt-token/src/amount.rs:34-38`

**Description:**  
The `Amount::molt()` function uses floating-point multiplication which can introduce precision errors:

```rust
pub fn molt(amount: f64) -> Self {
    assert!(amount >= 0.0, "amount must be non-negative");
    let lamports = (amount * LAMPORTS_PER_MOLT as f64).round() as u64;
    Self { lamports }
}
```

**Impact:**  
- Potential rounding errors for certain decimal values (e.g., 0.1 + 0.2 ≠ 0.3 in f64)
- Not a vulnerability, but could cause unexpected behavior

**Recommendation:**  
For financial applications, consider using a decimal library or only accepting string input that's parsed with fixed-point arithmetic (like the `molt-core` Amount type already does correctly).

---

### LOW-2: Config File Reading Without Size Limit

**Severity:** Low  
**Location:** `crates/clawnode/src/config.rs:79-86`

**Description:**  
Config file reading uses `read_to_string` without checking file size:

```rust
pub fn from_file(path: impl AsRef<Path>) -> Result<Self, NodeError> {
    let content = std::fs::read_to_string(path.as_ref())?;  // No size check
    Self::from_toml(&content)
}
```

**Impact:**  
- Could read extremely large files into memory
- Limited impact since config files are user-controlled

**Recommendation:**
```rust
const MAX_CONFIG_SIZE: u64 = 1_048_576; // 1 MiB

pub fn from_file(path: impl AsRef<Path>) -> Result<Self, NodeError> {
    let metadata = std::fs::metadata(path.as_ref())?;
    if metadata.len() > MAX_CONFIG_SIZE {
        return Err(NodeError::Config("config file too large".into()));
    }
    let content = std::fs::read_to_string(path.as_ref())?;
    Self::from_toml(&content)
}
```

---

### LOW-3: Escrow Fee Calculation Uses Floating Point

**Severity:** Low  
**Location:** `crates/molt-token/src/escrow.rs:147-154`

**Description:**  
Fee calculation uses floating-point arithmetic:

```rust
pub fn provider_payout(&self) -> Amount {
    let fee_lamports = (self.amount.lamports() as f64 * self.fee_rate) as u64;
    Amount::from_lamports(self.amount.lamports().saturating_sub(fee_lamports))
}
```

**Impact:**  
- Minor precision inconsistencies possible
- Could lose or gain fractional lamports

**Recommendation:**  
Use integer arithmetic with basis points:
```rust
// Store fee_rate as basis points (1/10000)
pub fn provider_payout(&self) -> Amount {
    let fee_basis_points = (self.fee_rate * 10000.0) as u64;
    let fee_lamports = self.amount.lamports() * fee_basis_points / 10000;
    Amount::from_lamports(self.amount.lamports().saturating_sub(fee_lamports))
}
```

---

### LOW-4: ConnectionPool Max Connections Not Enforced at Accept Time

**Severity:** Low  
**Location:** `crates/molt-p2p/src/connection.rs:316-330`

**Description:**  
The connection pool enforces max connections when adding, but doesn't prevent acceptance:

```rust
pub fn add_connection(&mut self, conn: PeerConnection) -> Result<(), P2pError> {
    if self.is_full() {
        return Err(P2pError::Connection("pool is full".into()));
    }
    // ...
}
```

**Impact:**  
- Race condition could allow temporary overcommit
- Resources consumed before rejection

**Recommendation:**  
Check capacity before accepting new connections at the network layer, not just when adding to pool.

---

### LOW-5: Certificate Validation Time Skew Not Explicitly Handled

**Severity:** Low  
**Location:** `crates/claw-pki/src/validation.rs:29-41`

**Description:**  
Certificate validation uses current time without tolerance for clock skew:

```rust
pub fn is_expired(cert: &Certificate) -> bool {
    cert.not_after() < Utc::now()
}

pub fn is_not_yet_valid(cert: &Certificate) -> bool {
    cert.not_before() > Utc::now()
}
```

**Impact:**  
- Certificates may fail validation due to minor clock differences between nodes

**Recommendation:**
```rust
const CLOCK_SKEW_TOLERANCE: chrono::Duration = chrono::Duration::minutes(5);

pub fn is_expired(cert: &Certificate) -> bool {
    cert.not_after() + CLOCK_SKEW_TOLERANCE < Utc::now()
}
```

---

### INFO-1: Excellent Integer Overflow Protection in Amount Types

**Severity:** Info (Positive)  
**Location:** `crates/molt-core/src/amount.rs`

**Observation:**  
The `molt-core` Amount type demonstrates excellent overflow protection:

```rust
pub const fn checked_add(self, rhs: Self) -> Option<Self> {
    match self.0.checked_add(rhs.0) {
        Some(v) => Some(Self(v)),
        None => None,
    }
}

pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
    match self.0.checked_sub(rhs.0) {
        Some(v) => Some(Self(v)),
        None => None,
    }
}
```

Both `checked_*` and `saturating_*` variants are provided. This is a best practice.

---

### INFO-2: Good Input Validation Framework

**Severity:** Info (Positive)  
**Location:** `crates/claw-proto/src/validation.rs`

**Observation:**  
The validation module provides clear, explicit limits:

```rust
pub const MAX_MEMORY_MB: u64 = 1_024_000;  // 1 TB
pub const MAX_CPU_CORES: u32 = 1024;
pub const MAX_GPU_COUNT: u32 = 64;
```

The `ValidationResult` type allows collecting multiple errors, which is good UX.

---

### INFO-3: Node Name Validation Well-Implemented

**Severity:** Info (Positive)  
**Location:** `crates/clawnode/src/config.rs:110-128`

**Observation:**  
Node name validation includes:
- Empty check
- Length limit (64 chars)
- Character whitelist (alphanumeric, hyphen, underscore)

This prevents injection attacks in names that might be used in shell commands or log messages.

---

## Areas Not Applicable / Not Found

1. **SQL Injection:** No SQL database usage found; all storage appears to be in-memory or file-based
2. **Command Injection:** Container images are validated, but actual container execution wasn't in scope
3. **Path Traversal:** File operations use proper path validation
4. **SSRF:** No outbound HTTP requests based on user input found

---

## Summary of Recommendations

### Immediate Actions (High Priority)
1. Add message size limits to WebSocket handling in `claw-gateway-server`
2. Add size validation to P2P message deserialization

### Short-Term (Medium Priority)
3. Validate gossip wire protocol message types explicitly
4. Add bounds to `WorkloadLogs.lines` array
5. Fix `parse_percentage` edge cases

### Long-Term (Low Priority)
6. Consider migrating financial calculations from f64 to fixed-point
7. Add file size checks before reading configs
8. Add clock skew tolerance to certificate validation

---

## Appendix: Files Reviewed

| Crate | File | Focus |
|-------|------|-------|
| molt-core | amount.rs | Integer overflow protection |
| molt-token | amount.rs, escrow.rs | Financial calculations |
| molt-p2p | message.rs, gossip/message.rs, connection.rs, protocol.rs | Network input |
| claw-proto | messages.rs, validation.rs, workload.rs | Protocol validation |
| claw-gateway-server | session.rs, handlers.rs, config.rs | WebSocket handling |
| clawnode | config.rs, network.rs | Config parsing, mesh networking |
| claw-pki | validation.rs | Certificate parsing |
| claw-deploy | parser.rs | Natural language parsing |
| claw-secrets | types.rs | Secret ID validation |
| claw-metrics | types.rs | Metric name validation |

---

*Report generated by automated security analysis. Manual review recommended for critical systems.*
