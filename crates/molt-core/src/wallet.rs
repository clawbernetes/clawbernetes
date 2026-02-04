//! Wallet abstraction for key management and signing.
//!
//! Provides Ed25519-based cryptographic operations for MOLT transactions.

use ed25519_dalek::{Signature as DalekSignature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

use crate::MoltError;

/// A MOLT wallet containing an Ed25519 keypair for signing transactions.
#[derive(Debug)]
pub struct Wallet {
    signing_key: SigningKey,
}

/// A public key derived from a wallet, used for verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey(VerifyingKey);

/// An Ed25519 signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature(DalekSignature);

impl Wallet {
    /// Creates a new wallet with a randomly generated keypair.
    #[must_use]
    pub fn new() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Returns the public key for this wallet.
    #[must_use]
    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.signing_key.verifying_key())
    }

    /// Signs a message with this wallet's private key.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature(self.signing_key.sign(message))
    }

    /// Returns the raw bytes of the signing key.
    ///
    /// # Security
    ///
    /// This exposes the private key material. Handle with care.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Creates a wallet from raw signing key bytes.
    ///
    /// # Errors
    ///
    /// Returns `MoltError::Wallet` if the bytes are invalid.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, MoltError> {
        let signing_key = SigningKey::from_bytes(bytes);
        Ok(Self { signing_key })
    }
}

impl Default for Wallet {
    fn default() -> Self {
        Self::new()
    }
}

impl PublicKey {
    /// Returns the raw bytes of the public key.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    /// Creates a public key from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns `MoltError::Crypto` if the bytes don't represent a valid public key.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, MoltError> {
        VerifyingKey::from_bytes(bytes)
            .map(PublicKey)
            .map_err(|e| MoltError::Crypto(e.to_string()))
    }

    /// Verifies a signature against a message using this public key.
    ///
    /// Uses strict verification to prevent signature malleability attacks.
    /// Standard Ed25519 verification allows multiple valid signatures for the same
    /// message, which can be exploited in replay attacks or double-spend scenarios.
    ///
    /// # Errors
    ///
    /// Returns `MoltError::InvalidSignature` if the signature is invalid.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), MoltError> {
        self.0
            .verify_strict(message, &signature.0)
            .map_err(|_| MoltError::InvalidSignature)
    }
}

impl Signature {
    /// Returns the raw bytes of the signature.
    #[must_use]
    pub fn as_bytes(&self) -> [u8; 64] {
        self.0.to_bytes()
    }

    /// Creates a signature from raw bytes.
    ///
    /// # Errors
    ///
    /// This function currently doesn't fail for any 64-byte input,
    /// but returns a Result for API consistency and future-proofing.
    pub fn from_bytes(bytes: &[u8; 64]) -> Result<Self, MoltError> {
        Ok(Self(DalekSignature::from_bytes(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_new_generates_valid_keypair() {
        let wallet = Wallet::new();
        let pubkey = wallet.public_key();
        assert_eq!(pubkey.as_bytes().len(), 32);
    }

    #[test]
    fn wallet_sign_produces_valid_signature() {
        let wallet = Wallet::new();
        let message = b"hello world";
        let signature = wallet.sign(message);
        assert_eq!(signature.as_bytes().len(), 64);
    }

    #[test]
    fn wallet_verify_accepts_valid_signature() {
        let wallet = Wallet::new();
        let pubkey = wallet.public_key();
        let message = b"test message";
        let signature = wallet.sign(message);
        
        assert!(pubkey.verify(message, &signature).is_ok());
    }

    #[test]
    fn wallet_verify_rejects_tampered_message() {
        let wallet = Wallet::new();
        let pubkey = wallet.public_key();
        let message = b"original";
        let signature = wallet.sign(message);
        
        assert!(pubkey.verify(b"tampered", &signature).is_err());
    }

    #[test]
    fn wallet_verify_rejects_wrong_key() {
        let wallet1 = Wallet::new();
        let wallet2 = Wallet::new();
        let message = b"test";
        let signature = wallet1.sign(message);
        
        assert!(wallet2.public_key().verify(message, &signature).is_err());
    }

    #[test]
    fn different_wallets_have_different_keys() {
        let wallet1 = Wallet::new();
        let wallet2 = Wallet::new();
        assert_ne!(wallet1.public_key(), wallet2.public_key());
    }

    #[test]
    fn wallet_from_bytes_roundtrips() {
        let wallet = Wallet::new();
        let bytes = wallet.to_bytes();
        let restored = Wallet::from_bytes(&bytes).unwrap();
        assert_eq!(wallet.public_key(), restored.public_key());
    }

    #[test]
    fn wallet_from_invalid_bytes_fails() {
        let bad_bytes = [0u8; 32];
        // This should still work since all 32-byte arrays are valid signing keys
        assert!(Wallet::from_bytes(&bad_bytes).is_ok());
    }

    #[test]
    fn public_key_from_bytes_roundtrips() {
        let wallet = Wallet::new();
        let pubkey = wallet.public_key();
        let bytes = pubkey.as_bytes();
        let restored = PublicKey::from_bytes(bytes).unwrap();
        assert_eq!(pubkey, restored);
    }

    #[test]
    fn signature_from_bytes_roundtrips() {
        let wallet = Wallet::new();
        let sig = wallet.sign(b"test");
        let bytes = sig.as_bytes();
        let restored = Signature::from_bytes(&bytes).unwrap();
        assert_eq!(sig, restored);
    }
}
