# MOLT Protocol Security Audit Report

**Audit Date:** February 4, 2026  
**Auditor:** Security Engineer (Subagent)  
**Repository:** clawbernetes  
**Components Reviewed:** molt-market, molt-attestation, molt-p2p, molt-agent

---

## Executive Summary

This security audit covers the MOLT marketplace protocol, including escrow logic, attestation systems, P2P gossip protocol, and agent autonomy policies. The audit identified several security concerns ranging from **Critical** to **Informational** severity.

**Overall Assessment:** The protocol demonstrates good security foundations with proper state machine design, cryptographic signing, and defense-in-depth patterns. However, several areas require attention before production deployment.

### Finding Summary

| Severity | Count |
|----------|-------|
| Critical | 1 |
| High | 3 |
| Medium | 5 |
| Low | 6 |
| Informational | 4 |

---

## Critical Findings

### CRIT-01: Missing Authorization Checks on Escrow State Transitions

**Component:** `molt-market/src/escrow.rs`  
**Severity:** Critical  
**Status:** Open

**Description:**

The `EscrowAccount` struct allows state transitions via `release()`, `refund()`, and `dispute()` methods without any authorization checks. Any caller with access to the `EscrowAccount` can transition its state.

```rust
/// Releases funds to the provider (job completed successfully).
pub fn release(&mut self) -> Result<(), MarketError> {
    self.transition_to(EscrowState::Released)
}
```

**Attack Scenario:**

1. Buyer creates escrow and funds it
2. Malicious actor (or compromised provider) calls `release()` before job completion
3. Funds are released without legitimate work being performed

**Impact:** Complete loss of escrowed funds for buyers.

**Mitigation:**

```rust
pub fn release(&mut self, caller: &str) -> Result<(), MarketError> {
    // Only the marketplace or authorized dispute resolver should release
    if caller != "marketplace" && caller != self.dispute_resolver {
        return Err(MarketError::Unauthorized);
    }
    self.transition_to(EscrowState::Released)
}
```

**Recommendation:**
- Add caller authorization to all state transition methods
- Implement role-based access control (marketplace, buyer, provider, arbitrator)
- Consider requiring multi-sig for high-value escrows

---

## High Severity Findings

### HIGH-01: No Rate Limiting on Gossip Message Processing

**Component:** `molt-p2p/src/gossip/broadcast.rs`  
**Severity:** High  
**Status:** Open

**Description:**

The `GossipBroadcaster` processes incoming messages without rate limiting per peer. While deduplication exists for message IDs, attackers can flood the network with unique messages.

```rust
pub fn handle_message(
    &mut self,
    message: &GossipMessage,
    from_peer: PeerId,
) -> Result<BroadcastResult, P2pError> {
    // No rate limiting per peer
    self.maybe_cleanup();
    // ... processes message
}
```

**Attack Scenario:**

1. Attacker connects to multiple network nodes
2. Floods nodes with unique `Announce` messages (each with different message IDs)
3. Causes resource exhaustion in seen_messages cache and announcement_cache
4. Network nodes become unresponsive (DoS)

**Impact:** Network-wide denial of service.

**Mitigation:**

```rust
struct RateLimiter {
    messages_per_peer: HashMap<PeerId, (Instant, u32)>,
    max_per_minute: u32,
}

impl RateLimiter {
    fn check(&mut self, peer: PeerId) -> bool {
        let entry = self.messages_per_peer.entry(peer).or_insert((Instant::now(), 0));
        if entry.0.elapsed() > Duration::from_secs(60) {
            *entry = (Instant::now(), 1);
            true
        } else if entry.1 < self.max_per_minute {
            entry.1 += 1;
            true
        } else {
            false // Rate limited
        }
    }
}
```

**Recommendation:**
- Implement per-peer rate limiting (messages/minute)
- Add bandwidth accounting per peer
- Implement exponential backoff for misbehaving peers
- Add peer reputation scoring with automatic disconnection

---

### HIGH-02: Attestation Replay Attack Across Nodes

**Component:** `molt-attestation/src/verification.rs`  
**Severity:** High  
**Status:** Open

**Description:**

Hardware and execution attestations are signed but contain no nonce or challenge-response mechanism. An attestation created for one verifier can be replayed to another.

```rust
pub fn verify_hardware_attestation(
    attestation: &HardwareAttestation,
    public_key: &VerifyingKey,
) -> Result<VerificationResult, AttestationError> {
    let not_expired = !attestation.is_expired();
    if !not_expired {
        return Err(AttestationError::Expired);
    }
    attestation.verify_signature(public_key)?;
    // No verifier-specific binding
    Ok(...)
}
```

**Attack Scenario:**

1. Node A creates valid hardware attestation and presents to Verifier 1
2. Attacker captures attestation (within validity period)
3. Attacker presents same attestation to Verifier 2, claiming to be Node A
4. Verifier 2 accepts attestation (no way to know it wasn't freshly generated)

**Impact:** Identity spoofing; malicious nodes can claim capabilities they don't have.

**Mitigation:**

```rust
fn create_signing_message(
    node_id: Uuid,
    gpus: &[GpuInfo],
    timestamp: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    challenge: &[u8; 32],  // Add challenge/nonce
    verifier_id: &[u8; 32], // Bind to specific verifier
) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"hardware_attestation_v2");
    hasher.update(challenge);
    hasher.update(verifier_id);
    // ... rest of message
}
```

**Recommendation:**
- Implement challenge-response for attestation requests
- Include verifier public key in signed attestation data
- Add nonce requirement for fresh attestations

---

### HIGH-03: Settlement Integer Precision Loss

**Component:** `molt-market/src/settlement.rs`  
**Severity:** High  
**Status:** Open

**Description:**

The `calculate_payment` function uses integer arithmetic with scaling that can cause precision loss, especially for short jobs or high rates.

```rust
pub const fn calculate_payment(duration_seconds: u64, rate_per_hour: u64) -> u64 {
    // Calculate payment: (duration / 3600) * rate
    let hours_scaled = duration_seconds * 1000 / 3600; // Scale up for precision
    (hours_scaled * rate_per_hour) / 1000
}
```

**Attack Scenario:**

1. Provider offers rate of 1000 tokens/hour
2. Job runs for 3 seconds
3. Expected payment: 3/3600 * 1000 = 0.833 tokens
4. Actual calculation: (3 * 1000 / 3600) * 1000 / 1000 = 0 * 1000 / 1000 = 0
5. Provider receives nothing for 3 seconds of work

Over many micro-jobs, this can result in significant underpayment.

**Impact:** Systematic underpayment for short jobs; providers lose revenue.

**Mitigation:**

```rust
pub fn calculate_payment(duration_seconds: u64, rate_per_hour: u64) -> u64 {
    // Use u128 for intermediate calculation to prevent overflow
    let numerator = (duration_seconds as u128) * (rate_per_hour as u128);
    let payment = numerator / 3600;
    
    // Optional: round up to ensure providers get minimum compensation
    if payment == 0 && duration_seconds > 0 && rate_per_hour > 0 {
        return 1; // Minimum 1 token for any work
    }
    
    payment.min(u64::MAX as u128) as u64
}
```

**Recommendation:**
- Use u128 for intermediate calculations
- Consider using fixed-point decimal libraries
- Add minimum payment threshold for non-zero work
- Add comprehensive edge-case tests for payment calculations

---

## Medium Severity Findings

### MED-01: No Signature Verification on Capacity Announcements in Orderbook

**Component:** `molt-market/src/orderbook.rs`  
**Severity:** Medium  
**Status:** Open

**Description:**

The `CapacityOffer` in the orderbook does not include cryptographic signatures. Offers can be forged or modified in transit.

```rust
pub struct CapacityOffer {
    pub id: String,
    pub provider: String,  // Just a string identifier, not cryptographically verified
    pub gpus: GpuCapacity,
    pub price_per_hour: u64,
    pub reputation: u32,
}
```

**Attack Scenario:**

1. Attacker intercepts legitimate offer from Provider A
2. Modifies `provider` field to point to attacker's address
3. Buyer accepts "offer" and sends payment to attacker

**Recommendation:**
- Add Ed25519 signature to CapacityOffer
- Verify signature before inserting into orderbook
- Include provider public key in offer for verification

---

### MED-02: Unbounded Announcement Cache Growth

**Component:** `molt-p2p/src/gossip/broadcast.rs`  
**Severity:** Medium  
**Status:** Open

**Description:**

While `max_announcements_per_peer` limits announcements per peer, the total number of unique peers is unbounded. An attacker can create many identities to fill the cache.

```rust
fn cache_announcement(&mut self, announcement: CapacityAnnouncement) {
    let peer_id = announcement.peer_id();
    let entries = self.announcement_cache.entry(peer_id).or_default();
    // Limits per peer, but not total peers
    while entries.len() > self.config.max_announcements_per_peer {
        entries.remove(0);
    }
}
```

**Attack Scenario:**

1. Attacker generates thousands of key pairs (Sybil attack)
2. Sends announcements from each identity
3. Cache grows unboundedly, consuming memory
4. Eventually causes OOM or performance degradation

**Recommendation:**
- Add `max_unique_providers` limit to cache
- Implement LRU eviction for oldest providers
- Consider stake-weighted caching priority

---

### MED-03: Spending Tracker Not Persisted

**Component:** `molt-agent/src/autonomy/mod.rs`  
**Severity:** Medium  
**Status:** Open

**Description:**

The `SpendingTracker` is an in-memory structure. If the agent restarts, spending history is lost, allowing the hourly budget to be bypassed.

```rust
pub struct SpendingTracker {
    hourly_budget: u64,
    spent_this_hour: u64,  // Lost on restart
}
```

**Attack Scenario:**

1. Agent has hourly budget of 10,000 tokens
2. Agent spends 9,500 tokens
3. Agent crashes/restarts
4. `spent_this_hour` resets to 0
5. Agent can spend another 10,000 tokens in the same hour

**Recommendation:**
- Persist spending tracker to disk with timestamps
- Load previous state on startup
- Calculate remaining budget based on time windows

---

### MED-04: Trust Score Manipulation via Selective Attestation

**Component:** `molt-attestation/src/hardware.rs`  
**Severity:** Medium  
**Status:** Open

**Description:**

The `AttestationChain::trust_score()` calculation weights recent verifications more heavily. Attackers can manipulate scores by timing attestation attempts.

```rust
pub fn trust_score(&self) -> f64 {
    // Later entries weighted more
    let weight = (i + 1) as f64 / total;
    if entry.verification_passed { weight } else { 0.0 }
}
```

**Attack Scenario:**

1. Attacker accumulates some failed verifications (using real hardware temporarily)
2. Once legitimate hardware is obtained, recent successful attestations quickly raise score
3. Trust score appears high despite history of failures

**Recommendation:**
- Add decay factor for old failures (but don't eliminate them)
- Consider total history length in scoring
- Implement minimum time-on-chain requirement

---

### MED-05: Eclipse Attack Vulnerability in Peer Discovery

**Component:** `molt-p2p/src/network.rs`  
**Severity:** Medium  
**Status:** Open

**Description:**

The network uses bootstrap nodes for initial peer discovery. If all bootstrap nodes are malicious or compromised, a new node can be eclipsed.

```rust
pub async fn join(&self, bootstrap_nodes: &[String]) -> Result<(), P2pError> {
    if bootstrap_nodes.is_empty() {
        // We become the first node
        inner.state = NetworkState::Online;
        return Ok(());
    }
    // Connects only to provided bootstraps
    for node_addr in bootstrap_nodes { ... }
}
```

**Attack Scenario:**

1. Attacker controls bootstrap nodes or performs DNS hijacking
2. New node joins and only connects to attacker-controlled peers
3. Attacker controls all information the victim sees
4. Can feed false job offers, manipulate market prices

**Recommendation:**
- Require multiple independent bootstrap sources
- Implement peer exchange protocol with random peer selection
- Add hardcoded fallback peers
- Implement anomaly detection for peer suggestions

---

## Low Severity Findings

### LOW-01: No Timeout on Escrow Funded State

**Component:** `molt-market/src/escrow.rs`  
**Severity:** Low  
**Status:** Open

**Description:**

An escrow can remain in `Funded` state indefinitely with no automatic expiration. Funds can be locked forever if a job never completes and no dispute is raised.

**Recommendation:**
- Add `funded_at` timestamp to EscrowAccount
- Implement automatic refund after timeout (e.g., 30 days)
- Allow buyer-initiated timeout claims

---

### LOW-02: Deterministic Fanout Selection Reveals Peer Information

**Component:** `molt-p2p/src/gossip/broadcast.rs`  
**Severity:** Low  
**Status:** Open

**Description:**

When fewer peers exist than the fanout factor, all peers are selected deterministically. This leaks information about the node's peer set.

```rust
if candidates.len() <= self.config.fanout {
    return candidates;  // Returns all peers
}
```

**Recommendation:**
- Always use random selection even with fewer peers
- Consider dummy padding for privacy

---

### LOW-03: Missing Input Validation on Job Requirements

**Component:** `molt-market/src/orderbook.rs`  
**Severity:** Low  
**Status:** Open

**Description:**

`JobRequirements` accepts any values without validation. Zero or extremely large values can cause issues.

```rust
pub struct JobRequirements {
    pub min_gpus: u32,           // Can be 0
    pub min_memory_gb: u32,      // Can be 0
    pub max_duration_hours: u32, // Can be u32::MAX
}
```

**Recommendation:**
- Add validation methods
- Reject obviously invalid requirements (0 GPUs, 0 memory)
- Add reasonable maximum bounds

---

### LOW-04: Autonomy Policy Bounds Not Enforced Atomically

**Component:** `molt-agent/src/autonomy/mod.rs`  
**Severity:** Low  
**Status:** Open

**Description:**

`PolicyBounds` checks are independent. Racing requests could exceed combined limits (e.g., concurrent job count + spending limit).

**Recommendation:**
- Implement atomic policy evaluation
- Use transaction-like commit/rollback pattern
- Add lock-free accounting structures

---

### LOW-05: No Verification of GPU Information in Announcements

**Component:** `molt-p2p/src/gossip/announcement.rs`  
**Severity:** Low  
**Status:** Open

**Description:**

Capacity announcements contain self-reported GPU information. There's no link to hardware attestation data.

**Recommendation:**
- Require attestation reference in announcements
- Cross-verify announced capabilities with attestations
- Implement proof-of-hardware mechanism

---

### LOW-06: Query Filter Matching is Case-Sensitive

**Component:** `molt-p2p/src/network.rs`  
**Severity:** Low  
**Status:** Open

**Description:**

The GPU model matching uses `to_lowercase()` but job type matching doesn't. Inconsistent behavior could cause missed matches.

```rust
// GPU model: case-insensitive
let has_model = ann.gpus().iter()
    .any(|gpu| gpu.model.to_lowercase().contains(&model.to_lowercase()));

// Job type: case-sensitive
let has_job_type = ann.job_types().iter().any(|jt| jt == job_type);
```

**Recommendation:**
- Make all string matching consistently case-insensitive
- Document expected casing conventions

---

## Informational Findings

### INFO-01: Hardcoded Magic Numbers in Protocol

**Component:** Multiple  
**Severity:** Informational

Several hardcoded values should be configurable:
- Default fanout: 3
- Default TTL hops: 6
- Seen cache size: 10,000
- Announcement cache TTL: 600 seconds

**Recommendation:** Move to configuration with sensible defaults.

---

### INFO-02: Missing Audit Logging

**Component:** All  
**Severity:** Informational

No audit trail for security-relevant operations:
- Escrow state changes
- Attestation verifications
- Policy evaluations
- Peer connections/disconnections

**Recommendation:** Add structured logging for security events.

---

### INFO-03: Test Coverage Gaps

**Component:** molt-market  
**Severity:** Informational

Missing tests for:
- Concurrent escrow operations
- Settlement with boundary values (u64::MAX)
- Orderbook with duplicate entries

**Recommendation:** Add fuzzing and property-based tests.

---

### INFO-04: No Version Negotiation in Protocol

**Component:** molt-p2p  
**Severity:** Informational

Message format includes version strings ("hardware_attestation_v1") but no protocol version negotiation.

**Recommendation:** Implement handshake with version negotiation for future compatibility.

---

## Recommendations Summary

### Immediate Actions (Pre-Production)
1. **CRIT-01**: Add authorization to escrow state transitions
2. **HIGH-01**: Implement per-peer rate limiting
3. **HIGH-02**: Add challenge-response to attestations
4. **HIGH-03**: Fix settlement precision loss

### Short-Term (Before Public Launch)
1. **MED-01**: Add signatures to capacity offers
2. **MED-02**: Bound total cache size
3. **MED-03**: Persist spending tracker
4. **MED-04**: Improve trust score algorithm
5. **MED-05**: Harden peer discovery

### Long-Term (Ongoing)
1. Implement comprehensive audit logging
2. Add fuzzing to CI/CD pipeline
3. Consider formal verification for escrow logic
4. Conduct external security audit before mainnet

---

## Appendix A: Test Recommendations

```rust
// Recommended property-based tests

#[test]
fn prop_settlement_never_loses_precision_significantly() {
    proptest!(|(duration in 1u64..86400, rate in 1u64..1_000_000)| {
        let payment = calculate_payment(duration, rate);
        let expected = (duration as f64 / 3600.0) * rate as f64;
        let error = (payment as f64 - expected).abs();
        assert!(error < 1.0, "Precision loss > 1 token");
    });
}

#[test]
fn prop_rate_limiter_enforces_bounds() {
    proptest!(|(messages in 0usize..1000, limit in 1usize..100)| {
        let mut limiter = RateLimiter::new(limit);
        let peer = PeerId::random();
        let accepted = (0..messages).filter(|_| limiter.check(peer)).count();
        assert!(accepted <= limit);
    });
}
```

---

## Appendix B: Threat Model

### Adversary Capabilities
- Control of network peers (up to 33% assumed)
- Ability to create Sybil identities
- Message interception (but not modification due to TLS)
- Temporary control of legitimate hardware

### Trust Assumptions
- Cryptographic primitives (Ed25519, Blake3) are secure
- Bootstrap nodes are initially trustworthy
- Hardware attestation root of trust is not compromised

### Out of Scope
- Side-channel attacks on GPU execution
- Physical attacks on provider hardware
- Social engineering of operators
- Smart contract vulnerabilities (on-chain)

---

*Report generated by security-molt-audit subagent*
