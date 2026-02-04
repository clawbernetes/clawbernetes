# Cryptographic Security Audit Report

**Project:** Clawbernetes  
**Audit Date:** 2026-02-04  
**Auditor:** Security Engineer (Automated Audit)  
**Scope:** All cryptographic operations in Rust crates

---

## Executive Summary

This audit reviewed all cryptographic code in the Clawbernetes project for security issues related to key generation, signing/verification, hashing, and random number generation. The codebase demonstrates generally good cryptographic practices with a few areas requiring attention.

| Severity | Count |
|----------|-------|
| Critical | 0 |
| High | 0 |
| Medium | 2 |
| Low | 2 |
| Info | 5 |

---

## Findings

### MEDIUM-01: Use of `thread_rng()` for Cryptographic Key Material

**Severity:** Medium  
**Status:** Open  
**Affected Files:**
- `crates/molt-token/src/wallet.rs:91`
- `crates/claw-secrets/src/encryption.rs:41`
- `crates/claw-secrets/src/encryption.rs:101`
- `crates/claw-wireguard/src/types.rs:47`

**Description:**

Several locations use `rand::thread_rng()` instead of `OsRng` for generating cryptographic key material. While `thread_rng()` is cryptographically secure on most platforms, `OsRng` provides a more direct interface to the operating system's cryptographic random number generator and is the recommended choice for security-critical operations.

**Affected Code:**

```rust
// molt-token/src/wallet.rs:90-92
pub fn generate() -> Result<Self> {
    let mut secret_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret_bytes);  // Should use OsRng
    let signing_key = SigningKey::from_bytes(&secret_bytes);
    ...
}

// claw-secrets/src/encryption.rs:39-42
pub fn generate() -> Self {
    let mut bytes = [0u8; KEY_SIZE];
    rand::thread_rng().fill_bytes(&mut bytes);  // Should use OsRng
    Self { bytes }
}

// claw-secrets/src/encryption.rs:100-101
let mut nonce_bytes = [0u8; NONCE_SIZE];
rand::thread_rng().fill_bytes(&mut nonce_bytes);  // Should use OsRng

// claw-wireguard/src/types.rs:45-48
pub fn generate() -> Self {
    use rand::RngCore;
    let mut key = [0u8; KEY_SIZE];
    rand::thread_rng().fill_bytes(&mut key);  // Should use OsRng
    Self(key)
}
```

**Comparison with Good Practice:**

The `molt-core/src/wallet.rs` correctly uses `OsRng`:
```rust
// molt-core/src/wallet.rs:27-28 (GOOD)
pub fn new() -> Self {
    let signing_key = SigningKey::generate(&mut OsRng);
    Self { signing_key }
}
```

**Recommendation:**

Replace all instances of `thread_rng()` with `OsRng` for cryptographic key material:

```rust
use rand::rngs::OsRng;

// For direct fill
OsRng.fill_bytes(&mut bytes);

// For key generation
let signing_key = SigningKey::generate(&mut OsRng);
```

**Risk Assessment:**
- `thread_rng()` internally uses `OsRng` for seeding, but adds a layer of indirection
- On exotic platforms, `thread_rng()` behavior may differ
- Using `OsRng` directly is more auditable and follows best practices

---

### MEDIUM-02: Inconsistent Signature Verification Method

**Severity:** Medium  
**Status:** Open  
**Affected Files:**
- `crates/molt-core/src/wallet.rs:96`
- `crates/molt-p2p/src/gossip/announcement.rs:174`
- `crates/molt-attestation/src/hardware.rs:158`
- `crates/molt-attestation/src/execution.rs:186`

**Description:**

The codebase uses `verify()` in production code but tests use `verify_strict()`. Ed25519 signatures can have malleability issues where the same message can have multiple valid signatures. The `verify_strict()` method performs additional checks to prevent signature malleability.

**Affected Code:**

```rust
// Production code uses verify() - molt-p2p/src/gossip/announcement.rs:174
verifying_key
    .verify(&message, &signature)
    .map_err(|e| P2pError::Protocol(format!("Invalid signature: {e}")))

// Test code uses verify_strict() - molt-token/src/wallet.rs:277
assert!(public_key.verify_strict(message, &signature).is_ok());
```

**Recommendation:**

Use `verify_strict()` consistently in all signature verification:

```rust
verifying_key
    .verify_strict(&message, &signature)
    .map_err(|e| P2pError::Protocol(format!("Invalid signature: {e}")))
```

---

### LOW-01: Missing Zeroization on Some Secret Key Types

**Severity:** Low  
**Status:** Open  
**Affected Files:**
- `crates/molt-core/src/wallet.rs:12`
- `crates/molt-token/src/wallet.rs:79`

**Description:**

The `Wallet` structs containing signing keys do not implement `Zeroize` or `ZeroizeOnDrop` traits. When these structs are dropped, the secret key material may remain in memory.

**Good Practice Found:**

```rust
// claw-secrets/src/encryption.rs:33-35 (GOOD)
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey {
    bytes: [u8; KEY_SIZE],
}
```

**Recommendation:**

Add zeroization to wallet types:

```rust
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Wallet {
    signing_key: SigningKey,
}
```

---

### LOW-02: Constant-Time Comparison Not Used Everywhere

**Severity:** Low  
**Status:** Open  
**Affected Files:**
- Various signature and key comparison locations

**Description:**

While some secret types correctly use constant-time comparison (good!), not all comparisons of cryptographic material use constant-time operations.

**Good Practices Found:**

```rust
// claw-wireguard/src/keys.rs:225-227 (GOOD)
impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

// claw-secrets/src/types.rs:187-188 (GOOD)
fn eq(&self, other: &Self) -> bool {
    use subtle::ConstantTimeEq;
    self.data.ct_eq(&other.data).into()
}
```

**Recommendation:**

Ensure all secret key comparisons use `subtle::ConstantTimeEq`.

---

### INFO-01: Proper Domain Separation in Hash Functions ✓

**Severity:** Info (Positive Finding)  
**Status:** Good

**Description:**

The codebase correctly implements domain separation for blake3 hashing by using unique context strings for different purposes:

```rust
// crates/molt-attestation/src/hardware.rs:179-180
let mut hasher = blake3::Hasher::new();
hasher.update(b"hardware_attestation_v1");

// crates/molt-attestation/src/execution.rs:212-213
let mut hasher = blake3::Hasher::new();
hasher.update(b"execution_attestation_v1");

// crates/claw-secrets/src/encryption.rs:73-74
let context = format!("claw-secrets v1 {}", secret_id.as_str());
let derived = blake3::derive_key(&context, &self.bytes);
```

This prevents cross-protocol attacks where a hash from one context could be reused in another.

---

### INFO-02: Correct Use of blake3 (No Length Extension Vulnerability) ✓

**Severity:** Info (Positive Finding)  
**Status:** Good

**Description:**

The project uses blake3 for hashing, which is immune to length extension attacks (unlike SHA-256 or SHA-512). This is a good choice for cryptographic hashing.

---

### INFO-03: No Hardcoded Keys in Production Code ✓

**Severity:** Info (Positive Finding)  
**Status:** Good

**Description:**

Hardcoded key patterns like `[0u8; 32]` or `[1u8; KEY_SIZE]` were only found in test code, not in production paths. This is correct behavior.

**Test Code (Acceptable):**
```rust
// crates/claw-wireguard/src/interface.rs:290 (in test)
PrivateKey::from_bytes(&[1u8; KEY_SIZE]).expect("valid key")
```

---

### INFO-04: Proper AEAD Usage ✓

**Severity:** Info (Positive Finding)  
**Status:** Good

**Description:**

The `claw-secrets` crate correctly uses ChaCha20-Poly1305 AEAD with:
- Random nonces for each encryption
- Proper nonce size (96 bits / 12 bytes)
- Authentication tag verification on decryption

```rust
// crates/claw-secrets/src/encryption.rs
// Correct format: nonce || ciphertext || tag
```

---

### INFO-05: Debug Output Redacts Secrets ✓

**Severity:** Info (Positive Finding)  
**Status:** Good

**Description:**

Secret types correctly redact their contents in Debug output:

```rust
// crates/claw-wireguard/src/keys.rs:218-220
impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrivateKey([REDACTED])")
    }
}

// crates/molt-token/src/wallet.rs:215-220
impl fmt::Debug for Wallet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wallet")
            .field("address", &self.address)
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}
```

---

## Summary Statistics

### Files Reviewed

| Crate | Files | Crypto-Relevant |
|-------|-------|-----------------|
| molt-core | 5+ | wallet.rs |
| molt-token | 5+ | wallet.rs |
| molt-attestation | 5+ | hardware.rs, execution.rs, verification.rs |
| molt-p2p | 10+ | network.rs, gossip/*.rs, announcement.rs |
| claw-secrets | 5+ | encryption.rs, types.rs |
| claw-wireguard | 5+ | keys.rs, types.rs |

### Cryptographic Primitives Used

| Primitive | Library | Usage | Status |
|-----------|---------|-------|--------|
| Ed25519 | ed25519-dalek | Signing/Verification | ✓ Good |
| X25519 | x25519-dalek | WireGuard Key Exchange | ✓ Good |
| ChaCha20-Poly1305 | chacha20poly1305 | Secret Encryption | ✓ Good |
| BLAKE3 | blake3 | Hashing, Key Derivation | ✓ Good |
| OsRng | rand | Random Generation | ⚠️ Inconsistent |

### Patterns Analysis

| Pattern | Count | Status |
|---------|-------|--------|
| SigningKey::generate(&mut OsRng) | 35+ | ✓ Good (mostly tests) |
| thread_rng().fill_bytes() | 4 | ⚠️ Needs review |
| verify() | 5+ | ⚠️ Consider verify_strict() |
| verify_strict() | 4 (tests only) | ✓ Good |
| Zeroize/ZeroizeOnDrop | 3 | ⚠️ Incomplete coverage |
| ct_eq (constant-time) | 3 | ✓ Good where used |

---

## Recommendations Summary

1. **Replace `thread_rng()` with `OsRng`** for all cryptographic key generation and nonce generation
2. **Use `verify_strict()`** instead of `verify()` for Ed25519 signature verification
3. **Add Zeroize trait** to wallet types containing secret keys
4. **Ensure constant-time comparison** for all secret material comparisons
5. **Continue good practices** with domain separation and AEAD usage

---

## Appendix: Files with Cryptographic Code

### Key Generation
- `crates/molt-core/src/wallet.rs` - Ed25519 wallet (uses OsRng ✓)
- `crates/molt-token/src/wallet.rs` - Token wallet (uses thread_rng ⚠️)
- `crates/claw-secrets/src/encryption.rs` - AES key generation (uses thread_rng ⚠️)
- `crates/claw-wireguard/src/keys.rs` - WireGuard keys (uses OsRng ✓)
- `crates/claw-wireguard/src/types.rs` - Preshared keys (uses thread_rng ⚠️)

### Signing & Verification
- `crates/molt-attestation/src/hardware.rs` - Hardware attestation signing
- `crates/molt-attestation/src/execution.rs` - Execution attestation signing
- `crates/molt-p2p/src/gossip/announcement.rs` - Capacity announcement signing
- `crates/molt-p2p/src/network.rs` - Network message authentication

### Hashing
- `crates/molt-attestation/src/hardware.rs` - blake3 for attestation hashing
- `crates/molt-attestation/src/execution.rs` - blake3 for checkpoint hashing
- `crates/claw-secrets/src/encryption.rs` - blake3 for key derivation

---

*Report generated automatically. Manual review recommended for critical systems.*
