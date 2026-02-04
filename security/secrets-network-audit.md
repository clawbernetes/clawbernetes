# Secrets & Network Security Audit Report

**Project:** Clawbernetes  
**Audit Date:** 2026-02-04  
**Auditor:** Security Engineer (Automated Audit)  
**Scope:** Secrets management, TLS/certificate handling, network security, authentication

---

## Executive Summary

The Clawbernetes project demonstrates **strong security practices** in secrets management and cryptographic operations. The codebase shows evidence of security-conscious design with proper use of:
- Memory-safe secret handling with zeroization
- Constant-time comparisons to prevent timing attacks
- Debug redaction for sensitive types
- Structured encryption using modern algorithms (ChaCha20-Poly1305)

However, there are areas for improvement, particularly around TLS configuration enforcement and some minor logging concerns.

---

## Findings

### 🔴 CRITICAL

**No critical findings.**

---

### 🟠 HIGH

#### H-1: WebSocket Connections May Allow Insecure Transport

**Location:** 
- `crates/clawnode/src/gateway/client.rs:91`
- `crates/clawnode/src/gateway/handle.rs:92`
- `crates/clawnode/src/gateway/auto_reconnect.rs:145`

**Description:**  
The WebSocket client uses `tokio_tungstenite::connect_async()` with `MaybeTlsStream`, which automatically negotiates TLS for `wss://` URLs. However, the codebase validates URLs accept both `ws://` and `wss://` schemes:

```rust
// crates/clawnode/src/config.rs:143-145
if !self.gateway_url.starts_with("ws://") && !self.gateway_url.starts_with("wss://") {
    return Err(ConfigError::InvalidConfig(
        "gateway_url must start with ws:// or wss://".to_string(),
```

This allows insecure `ws://` connections which transmit node registration, heartbeats, and potentially sensitive workload data in plaintext.

**Impact:** Medium-High - Sensitive data could be intercepted on untrusted networks.

**Recommendation:**  
1. In production configurations, enforce `wss://` only
2. Add a configuration option `require_tls: bool` (default `true` for production)
3. Log a warning when `ws://` is used in non-development environments

```rust
// Recommended change
if self.require_tls && self.gateway_url.starts_with("ws://") {
    return Err(ConfigError::InvalidConfig(
        "insecure ws:// not allowed when require_tls is enabled".to_string(),
    ));
}
```

---

### 🟡 MEDIUM

#### M-1: No Certificate Pinning for Gateway Connections

**Location:**  
- `crates/clawnode/src/gateway/client.rs`
- `crates/clawnode/src/gateway/auto_reconnect.rs`

**Description:**  
The WebSocket client relies on system trust store for TLS certificate validation. While `tokio-tungstenite` with native TLS handles certificate validation, there is no certificate pinning implemented for gateway connections.

**Impact:** In a supply chain attack or CA compromise scenario, MITM attacks would be possible.

**Recommendation:**  
1. Implement optional certificate pinning for production deployments
2. Allow configuration of trusted CA certificates for the gateway
3. Consider using the existing `claw-pki` crate for certificate validation:

```rust
// Example: Add to NodeConfig
pub struct NodeConfig {
    // ... existing fields
    pub gateway_ca_cert: Option<PathBuf>, // Pin to specific CA
    pub gateway_cert_fingerprint: Option<String>, // Or pin by fingerprint
}
```

---

#### M-2: Private Key File Permissions Not Enforced

**Location:**  
- `crates/clawnode/src/network.rs:168-175`

**Description:**  
When saving WireGuard private keys to disk, the code writes without restricting file permissions:

```rust
std::fs::write(path, keypair.private_key().to_base64())
    .map_err(|e| NodeError::Config(format!("failed to write key file: {e}")))?;
```

On Unix systems, this may create files with overly permissive modes (e.g., 0644).

**Impact:** Other users on shared systems could read private keys.

**Recommendation:**  
Set restrictive permissions (0600) when creating key files:

```rust
use std::os::unix::fs::OpenOptionsExt;

let mut file = std::fs::OpenOptions::new()
    .write(true)
    .create(true)
    .mode(0o600)  // Unix: owner read/write only
    .open(path)?;
file.write_all(keypair.private_key().to_base64().as_bytes())?;
```

---

#### M-3: Tailscale Auth Key Logged in Some Error Paths

**Location:**  
- `crates/claw-tailscale/src/auth.rs:216-220`

**Description:**  
When auth key resolution fails, error messages may include partial information about the auth source. While the actual key value is not logged, information disclosure through error messages could aid attackers:

```rust
let key = std::env::var(var_name).map_err(|_| {
    TailscaleError::auth_key_not_found(format!(
        "environment variable '{var_name}' not set"
    ))
})?;
```

**Impact:** Low-Medium - Information about authentication configuration could leak.

**Recommendation:**  
Use generic error messages in production, with detailed messages only at debug level.

---

### 🟢 LOW

#### L-1: Wallet Private Key Accessible via `secret_key()` Method

**Location:**  
- `crates/molt-token/src/wallet.rs:106-109`

**Description:**  
The `Wallet` struct exposes raw secret key bytes through a public method:

```rust
/// Get the secret key bytes (careful with this!).
#[must_use]
pub fn secret_key(&self) -> &[u8; 32] {
    self.signing_key.as_bytes()
}
```

While the comment warns users, this could lead to accidental exposure.

**Impact:** Low - Requires intentional misuse, but could lead to key leakage.

**Recommendation:**  
1. Consider making this method `pub(crate)` or requiring explicit feature flag
2. Return a wrapper type that doesn't implement `Debug` and requires explicit `expose_secret()` call

---

#### L-2: Session ID Logged Without Masking

**Location:**  
- `crates/claw-gateway-server/src/session.rs` (multiple locations)

**Description:**  
Session UUIDs are logged in full in various tracing calls. While UUIDs are not secrets, correlating them across logs could aid in tracking or enumeration.

```rust
info!(session_id = %session_id, "Starting session handler");
```

**Impact:** Low - Could aid in session enumeration or tracking.

**Recommendation:**  
Consider logging only partial session IDs (first 8 characters) for routine operations.

---

### ℹ️ INFO (Positive Findings)

#### I-1: Excellent Secret Value Handling ✅

**Location:** `crates/claw-secrets/src/types.rs`

The `SecretValue` type demonstrates security best practices:
- Uses `zeroize::ZeroizeOnDrop` for secure memory clearing
- Implements constant-time equality via `subtle::ConstantTimeEq`
- Debug output is redacted (`[REDACTED]`)

```rust
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretValue {
    data: Vec<u8>,
}

impl PartialEq for SecretValue {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        self.data.ct_eq(&other.data).into()
    }
}
```

---

#### I-2: Proper Key Derivation ✅

**Location:** `crates/claw-secrets/src/encryption.rs`

Secret-specific keys are derived using BLAKE3:
- Prevents key reuse across secrets
- Uses versioned context strings for domain separation

```rust
pub fn derive_for_secret(&self, secret_id: &SecretId) -> Self {
    let context = format!("claw-secrets v1 {}", secret_id.as_str());
    let derived = blake3::derive_key(&context, &self.bytes);
    Self { bytes: derived }
}
```

---

#### I-3: WireGuard Key Security ✅

**Location:** `crates/claw-wireguard/src/keys.rs`

- Private keys use constant-time comparison
- Debug output is redacted for private keys
- Keys are generated using cryptographically secure RNG (`OsRng`)

---

#### I-4: PKI Private Key Zeroization ✅

**Location:** `crates/claw-pki/src/types.rs`

Private keys in the PKI module use `ZeroizeOnDrop`:

```rust
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey {
    der: Vec<u8>,
}
```

---

#### I-5: Certificate Validation Implemented ✅

**Location:** `crates/claw-pki/src/validation.rs`

Proper certificate validation chain:
- Expiration checking
- Not-yet-valid checking
- Signature verification
- Issuer matching

---

#### I-6: No Hardcoded Credentials ✅

**Finding:** Grep search for hardcoded credentials patterns returned no matches.

---

#### I-7: Audit Logging for Secret Access ✅

**Location:** `crates/claw-secrets/src/audit.rs`

All secret access is logged with:
- Timestamp
- Accessor identity
- Action type (Created, Read, Updated, Deleted, Rotated, AccessDenied)
- Reason string

---

## Summary Table

| ID | Severity | Finding | Status |
|----|----------|---------|--------|
| H-1 | High | Insecure WebSocket transport allowed | Open |
| M-1 | Medium | No certificate pinning | Open |
| M-2 | Medium | Key file permissions not enforced | Open |
| M-3 | Medium | Auth config info in error messages | Open |
| L-1 | Low | Wallet secret key publicly accessible | Open |
| L-2 | Low | Full session IDs in logs | Open |
| I-1 | Info | Excellent SecretValue handling | ✅ Positive |
| I-2 | Info | Proper key derivation | ✅ Positive |
| I-3 | Info | WireGuard key security | ✅ Positive |
| I-4 | Info | PKI key zeroization | ✅ Positive |
| I-5 | Info | Certificate validation | ✅ Positive |
| I-6 | Info | No hardcoded credentials | ✅ Positive |
| I-7 | Info | Secret access audit logging | ✅ Positive |

---

## Recommendations Summary

### Immediate Actions (High Priority)
1. Enforce TLS (`wss://`) for production gateway connections
2. Add configuration option to require secure transport

### Short-term Actions (Medium Priority)
3. Set restrictive file permissions for key files (0600)
4. Implement optional certificate pinning for gateway connections
5. Sanitize error messages to not leak configuration details

### Long-term Actions (Lower Priority)
6. Consider restricting wallet secret key access
7. Mask session IDs in routine log messages
8. Add security configuration validation on startup

---

## Files Reviewed

- `crates/claw-secrets/src/*.rs` - Secrets management
- `crates/claw-wireguard/src/*.rs` - WireGuard integration
- `crates/claw-pki/src/*.rs` - Certificate handling
- `crates/clawnode/src/gateway/*.rs` - Gateway client
- `crates/clawnode/src/network.rs` - Network mesh
- `crates/clawnode/src/config.rs` - Node configuration
- `crates/claw-gateway-server/src/*.rs` - Gateway server
- `crates/molt-token/src/wallet.rs` - Wallet handling
- `crates/claw-tailscale/src/auth.rs` - Tailscale auth

---

*Report generated: 2026-02-04*
