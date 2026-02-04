//! Wallet management for MOLT tokens.
//!
//! Wallets use Ed25519 keypairs compatible with Solana.

use crate::error::{MoltError, Result};
use ed25519_dalek::{SigningKey, VerifyingKey, Signer, Signature};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

/// A Solana-compatible address (base58-encoded public key).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(String);

impl Address {
    /// Create an address from a base58-encoded string.
    ///
    /// # Errors
    ///
    /// Returns error if the string is not valid base58 or wrong length.
    pub fn from_base58(s: &str) -> Result<Self> {
        let bytes = bs58::decode(s).into_vec().map_err(|e| {
            MoltError::invalid_address(format!("invalid base58: {e}"))
        })?;

        if bytes.len() != 32 {
            return Err(MoltError::invalid_address(format!(
                "address must be 32 bytes, got {}",
                bytes.len()
            )));
        }

        Ok(Self(s.to_string()))
    }

    /// Create an address from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns error if bytes are not 32 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(MoltError::invalid_address(format!(
                "address must be 32 bytes, got {}",
                bytes.len()
            )));
        }
        Ok(Self(bs58::encode(bytes).into_string()))
    }

    /// Get the base58-encoded address string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the raw bytes of the address.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        bs58::decode(&self.0).into_vec().unwrap_or_default()
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Address {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// A MOLT wallet (Ed25519 keypair).
pub struct Wallet {
    signing_key: SigningKey,
    address: Address,
}

impl Wallet {
    /// Generate a new random wallet.
    ///
    /// Uses `OsRng` directly instead of `thread_rng()` because cryptographic
    /// key material should come directly from the operating system's CSPRNG
    /// rather than a userspace PRNG that is merely seeded from system entropy.
    ///
    /// # Errors
    ///
    /// Returns error if random generation fails.
    pub fn generate() -> Result<Self> {
        let mut secret_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut secret_bytes);
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        let address = Address::from_bytes(verifying_key.as_bytes())?;

        Ok(Self {
            signing_key,
            address,
        })
    }

    /// Create a wallet from a secret key (32 bytes).
    ///
    /// # Errors
    ///
    /// Returns error if the key is invalid.
    pub fn from_secret_key(secret: &[u8]) -> Result<Self> {
        if secret.len() != 32 {
            return Err(MoltError::WalletError {
                message: format!("secret key must be 32 bytes, got {}", secret.len()),
            });
        }

        let secret_array: [u8; 32] = secret.try_into().map_err(|_| MoltError::WalletError {
            message: "failed to convert secret key".to_string(),
        })?;

        let signing_key = SigningKey::from_bytes(&secret_array);
        let verifying_key = signing_key.verifying_key();
        let address = Address::from_bytes(verifying_key.as_bytes())?;

        Ok(Self {
            signing_key,
            address,
        })
    }

    /// Create a wallet from a base58-encoded secret key.
    ///
    /// # Errors
    ///
    /// Returns error if the key is invalid.
    pub fn from_base58_secret(secret: &str) -> Result<Self> {
        let bytes = bs58::decode(secret).into_vec().map_err(|e| {
            MoltError::WalletError {
                message: format!("invalid base58: {e}"),
            }
        })?;
        Self::from_secret_key(&bytes)
    }

    /// Load a wallet from a JSON file (Solana CLI format).
    ///
    /// # Errors
    ///
    /// Returns error if file doesn't exist or is invalid.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let bytes: Vec<u8> = serde_json::from_str(&contents)?;
        
        if bytes.len() != 64 {
            return Err(MoltError::WalletError {
                message: format!("wallet file must contain 64 bytes, got {}", bytes.len()),
            });
        }

        // Solana CLI format: first 32 bytes are secret, next 32 are public
        Self::from_secret_key(&bytes[..32])
    }

    /// Save the wallet to a JSON file (Solana CLI format).
    ///
    /// # Errors
    ///
    /// Returns error if file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(self.signing_key.as_bytes());
        bytes.extend_from_slice(self.signing_key.verifying_key().as_bytes());
        
        let json = serde_json::to_string(&bytes)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Get the wallet address.
    #[must_use]
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Get the public key (verifying key).
    #[must_use]
    pub fn public_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Get the secret key bytes (careful with this!).
    #[must_use]
    pub fn secret_key(&self) -> &[u8; 32] {
        self.signing_key.as_bytes()
    }

    /// Get the secret key as base58.
    #[must_use]
    pub fn secret_key_base58(&self) -> String {
        bs58::encode(self.signing_key.as_bytes()).into_string()
    }

    /// Sign a message.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Sign a message and return the signature as bytes.
    #[must_use]
    pub fn sign_bytes(&self, message: &[u8]) -> [u8; 64] {
        self.sign(message).to_bytes()
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl fmt::Debug for Wallet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wallet")
            .field("address", &self.address)
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_generate_wallet() {
        let wallet = Wallet::generate().expect("should generate");
        assert!(!wallet.address().as_str().is_empty());
    }

    #[test]
    fn test_address_roundtrip() {
        let wallet = Wallet::generate().expect("should generate");
        let addr_str = wallet.address().as_str();
        let parsed = Address::from_base58(addr_str).expect("should parse");
        assert_eq!(wallet.address(), &parsed);
    }

    #[test]
    fn test_secret_key_roundtrip() {
        let wallet1 = Wallet::generate().expect("should generate");
        let secret = wallet1.secret_key();
        let wallet2 = Wallet::from_secret_key(secret).expect("should create");
        assert_eq!(wallet1.address(), wallet2.address());
    }

    #[test]
    fn test_base58_secret_roundtrip() {
        let wallet1 = Wallet::generate().expect("should generate");
        let secret_b58 = wallet1.secret_key_base58();
        let wallet2 = Wallet::from_base58_secret(&secret_b58).expect("should create");
        assert_eq!(wallet1.address(), wallet2.address());
    }

    #[test]
    fn test_save_and_load() {
        let wallet1 = Wallet::generate().expect("should generate");
        let temp_file = NamedTempFile::new().expect("should create temp file");
        
        wallet1.save(temp_file.path()).expect("should save");
        let wallet2 = Wallet::from_file(temp_file.path()).expect("should load");
        
        assert_eq!(wallet1.address(), wallet2.address());
    }

    #[test]
    fn test_sign_message() {
        let wallet = Wallet::generate().expect("should generate");
        let message = b"hello MOLT";
        let signature = wallet.sign(message);
        
        // Verify signature
        let public_key = wallet.public_key();
        assert!(public_key.verify_strict(message, &signature).is_ok());
    }

    #[test]
    fn test_invalid_address() {
        let result = Address::from_base58("invalid!");
        assert!(result.is_err());
    }

    #[test]
    fn test_address_wrong_length() {
        // Valid base58 but wrong length
        let result = Address::from_base58("abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_wallet_debug_redacts_secret() {
        let wallet = Wallet::generate().expect("should generate");
        let debug = format!("{wallet:?}");
        assert!(debug.contains("REDACTED"));
        assert!(!debug.contains(&wallet.secret_key_base58()));
    }

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn test_multiple_wallet_generation() {
        let wallet1 = Wallet::generate().expect("should generate");
        let wallet2 = Wallet::generate().expect("should generate");
        // Two randomly generated wallets should have different addresses
        assert_ne!(wallet1.address(), wallet2.address());
    }

    #[test]
    fn test_address_display() {
        let wallet = Wallet::generate().expect("should generate");
        let display = format!("{}", wallet.address());
        // Base58 addresses are typically 32-44 characters
        assert!(display.len() >= 32);
        assert!(display.len() <= 50);
    }

    #[test]
    fn test_address_debug() {
        let wallet = Wallet::generate().expect("should generate");
        let debug = format!("{:?}", wallet.address());
        assert!(debug.contains("Address"));
    }

    #[test]
    fn test_address_clone() {
        let wallet = Wallet::generate().expect("should generate");
        let addr1 = wallet.address().clone();
        let addr2 = wallet.address().clone();
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn test_address_hash() {
        use std::collections::HashSet;
        let wallet1 = Wallet::generate().expect("should generate");
        let wallet2 = Wallet::generate().expect("should generate");
        
        let mut set = HashSet::new();
        set.insert(wallet1.address().clone());
        set.insert(wallet2.address().clone());
        set.insert(wallet1.address().clone()); // Duplicate
        
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_sign_different_messages() {
        let wallet = Wallet::generate().expect("should generate");
        let sig1 = wallet.sign(b"message1");
        let sig2 = wallet.sign(b"message2");
        // Different messages should produce different signatures
        assert_ne!(sig1.to_bytes(), sig2.to_bytes());
    }

    #[test]
    fn test_sign_empty_message() {
        let wallet = Wallet::generate().expect("should generate");
        let message = b"";
        let signature = wallet.sign(message);
        let public_key = wallet.public_key();
        assert!(public_key.verify_strict(message, &signature).is_ok());
    }

    #[test]
    fn test_sign_large_message() {
        let wallet = Wallet::generate().expect("should generate");
        let message = vec![0xABu8; 10000];
        let signature = wallet.sign(&message);
        let public_key = wallet.public_key();
        assert!(public_key.verify_strict(&message, &signature).is_ok());
    }

    #[test]
    fn test_wrong_signature_verification() {
        let wallet1 = Wallet::generate().expect("should generate");
        let wallet2 = Wallet::generate().expect("should generate");
        
        let message = b"test message";
        let signature = wallet1.sign(message);
        
        // Verify with wrong public key should fail
        let wrong_public_key = wallet2.public_key();
        assert!(wrong_public_key.verify_strict(message, &signature).is_err());
    }

    #[test]
    fn test_secret_key_length() {
        let wallet = Wallet::generate().expect("should generate");
        let secret = wallet.secret_key();
        // Ed25519 seed/secret is 32 bytes
        assert_eq!(secret.len(), 32);
    }

    #[test]
    fn test_from_file_not_found() {
        let result = Wallet::from_file("/nonexistent/path/wallet.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_secret_key_wrong_length() {
        // Wrong length (too short)
        let invalid = vec![0u8; 16];
        let result = Wallet::from_secret_key(&invalid);
        assert!(result.is_err());

        // Wrong length (too long)
        let invalid = vec![0u8; 64];
        let result = Wallet::from_secret_key(&invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_base58_secret() {
        let result = Wallet::from_base58_secret("not-valid-base58!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_address_serialization() {
        let wallet = Wallet::generate().expect("should generate");
        let addr = wallet.address();
        let json = serde_json::to_string(addr).expect("serialize");
        let parsed: Address = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(addr, &parsed);
    }

    #[test]
    fn test_public_key() {
        let wallet = Wallet::generate().expect("should generate");
        let pk = wallet.public_key();
        // Public key should be 32 bytes
        assert_eq!(pk.as_bytes().len(), 32);
    }

    #[test]
    fn test_wallet_recreation() {
        // Test that a wallet can be recreated from its secret key
        let wallet1 = Wallet::generate().expect("should generate");
        let secret = wallet1.secret_key_base58();
        let wallet2 = Wallet::from_base58_secret(&secret).expect("should create");
        
        // Both should have same address and sign identically
        assert_eq!(wallet1.address(), wallet2.address());
        let msg = b"test";
        assert_eq!(wallet1.sign(msg).to_bytes(), wallet2.sign(msg).to_bytes());
    }
}
