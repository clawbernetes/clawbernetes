//! `WireGuard` key types.
//!
//! `WireGuard` uses Curve25519 for key exchange. Keys are 32 bytes.

use crate::error::WireGuardError;
use base64::Engine;
use rand_core::OsRng;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

/// `WireGuard` key size in bytes (256-bit Curve25519 keys).
pub const KEY_SIZE: usize = 32;

/// A `WireGuard` public key (Curve25519, 32 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey([u8; KEY_SIZE]);

impl PublicKey {
    /// Creates a public key from raw bytes.
    #[must_use]
    pub const fn from_bytes_array(bytes: [u8; KEY_SIZE]) -> Self {
        Self(bytes)
    }

    /// Creates a public key from a byte slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 32 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WireGuardError> {
        if bytes.len() != KEY_SIZE {
            return Err(WireGuardError::InvalidKeyLength(bytes.len()));
        }
        let mut arr = [0u8; KEY_SIZE];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    /// Returns the raw bytes of the public key.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.0
    }

    /// Returns the bytes as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Encodes the key as base64.
    #[must_use]
    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.0)
    }

    /// Decodes a public key from base64.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid base64 or wrong length.
    pub fn from_base64(s: &str) -> Result<Self, WireGuardError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| WireGuardError::InvalidBase64(e.to_string()))?;
        Self::from_bytes(&bytes)
    }

    /// Encodes the key as base58.
    #[must_use]
    pub fn to_base58(&self) -> String {
        bs58::encode(&self.0).into_string()
    }

    /// Decodes a public key from base58.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid base58 or wrong length.
    pub fn from_base58(s: &str) -> Result<Self, WireGuardError> {
        let bytes = bs58::decode(s)
            .into_vec()
            .map_err(|e| WireGuardError::InvalidBase64(e.to_string()))?;
        Self::from_bytes(&bytes)
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b64 = self.to_base64();
        let short = &b64[..8.min(b64.len())];
        write!(f, "PublicKey({short}...)")
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_base58())
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_base58())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_base58(&s).map_err(serde::de::Error::custom)
    }
}

impl From<X25519PublicKey> for PublicKey {
    fn from(key: X25519PublicKey) -> Self {
        Self::from_bytes_array(*key.as_bytes())
    }
}

/// A `WireGuard` private key (Curve25519, 32 bytes).
#[derive(Clone)]
pub struct PrivateKey([u8; KEY_SIZE]);

impl PrivateKey {
    /// Generates a new random private key.
    #[must_use]
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        Self(secret.to_bytes())
    }

    /// Creates a private key from a 32-byte array.
    #[must_use]
    pub const fn from_bytes_array(bytes: [u8; KEY_SIZE]) -> Self {
        Self(bytes)
    }

    /// Creates a private key from a byte slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the slice is not exactly 32 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WireGuardError> {
        if bytes.len() != KEY_SIZE {
            return Err(WireGuardError::InvalidKeyLength(bytes.len()));
        }
        let mut arr = [0u8; KEY_SIZE];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    /// Returns the raw bytes of the private key.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.0
    }

    /// Derives the corresponding public key.
    #[must_use]
    pub fn public_key(&self) -> PublicKey {
        let secret = StaticSecret::from(self.0);
        let public = X25519PublicKey::from(&secret);
        PublicKey::from(public)
    }

    /// Derives the corresponding public key (alias for `public_key`).
    #[must_use]
    pub fn derive_public_key(&self) -> PublicKey {
        self.public_key()
    }

    /// Encodes the key as base64.
    #[must_use]
    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.0)
    }

    /// Decodes a private key from base64.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid base64 or wrong length.
    pub fn from_base64(s: &str) -> Result<Self, WireGuardError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| WireGuardError::InvalidBase64(e.to_string()))?;
        Self::from_bytes(&bytes)
    }

    /// Encodes the key as base58.
    #[must_use]
    pub fn to_base58(&self) -> String {
        bs58::encode(&self.0).into_string()
    }

    /// Decodes a private key from base58.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not valid base58 or wrong length.
    pub fn from_base58(s: &str) -> Result<Self, WireGuardError> {
        let bytes = bs58::decode(s)
            .into_vec()
            .map_err(|e| WireGuardError::InvalidBase64(e.to_string()))?;
        Self::from_bytes(&bytes)
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrivateKey([REDACTED])")
    }
}

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for PrivateKey {}

/// A `WireGuard` key pair (private + public).
#[derive(Clone)]
pub struct KeyPair {
    private: PrivateKey,
    public: PublicKey,
}

impl KeyPair {
    /// Generates a new random key pair.
    #[must_use]
    pub fn generate() -> Self {
        let private = PrivateKey::generate();
        let public = private.public_key();
        Self { private, public }
    }

    /// Creates a key pair from an existing private key.
    #[must_use]
    pub fn from_private_key(private: PrivateKey) -> Self {
        let public = private.public_key();
        Self { private, public }
    }

    /// Returns a reference to the private key.
    #[must_use]
    pub const fn private_key(&self) -> &PrivateKey {
        &self.private
    }

    /// Returns a reference to the public key.
    #[must_use]
    pub const fn public_key(&self) -> &PublicKey {
        &self.public
    }

    /// Consumes the key pair and returns the private key.
    #[must_use]
    pub fn into_private_key(self) -> PrivateKey {
        self.private
    }
}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair")
            .field("private", &"[REDACTED]")
            .field("public", &self.public)
            .finish()
    }
}

/// Generates a new `WireGuard` keypair.
#[must_use]
pub fn generate_keypair() -> (PrivateKey, PublicKey) {
    let private = PrivateKey::generate();
    let public = private.public_key();
    (private, public)
}

/// Derives a public key from a private key.
#[must_use]
#[allow(dead_code)]
pub fn public_key_from_private(private: &PrivateKey) -> PublicKey {
    private.public_key()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_key_generate_produces_valid_key() {
        let key = PrivateKey::generate();
        assert_eq!(key.as_bytes().len(), KEY_SIZE);
    }

    #[test]
    fn private_key_to_public_key_is_deterministic() {
        let private = PrivateKey::generate();
        let public1 = private.public_key();
        let public2 = private.public_key();
        assert_eq!(public1, public2);
    }

    #[test]
    fn different_private_keys_produce_different_public_keys() {
        let private1 = PrivateKey::generate();
        let private2 = PrivateKey::generate();
        assert_ne!(private1.public_key(), private2.public_key());
    }

    #[test]
    fn public_key_base64_roundtrip() {
        let private = PrivateKey::generate();
        let public = private.public_key();
        let encoded = public.to_base64();
        let decoded = PublicKey::from_base64(&encoded).expect("decode failed");
        assert_eq!(public, decoded);
    }

    #[test]
    fn public_key_base58_roundtrip() {
        let private = PrivateKey::generate();
        let public = private.public_key();
        let encoded = public.to_base58();
        let decoded = PublicKey::from_base58(&encoded).expect("decode failed");
        assert_eq!(public, decoded);
    }

    #[test]
    fn private_key_base64_roundtrip() {
        let private = PrivateKey::generate();
        let encoded = private.to_base64();
        let decoded = PrivateKey::from_base64(&encoded).expect("decode failed");
        assert_eq!(private, decoded);
    }

    #[test]
    fn private_key_debug_redacts() {
        let private = PrivateKey::generate();
        let debug = format!("{private:?}");
        assert!(debug.contains("REDACTED"));
    }

    #[test]
    fn public_key_serde_roundtrip() {
        let private = PrivateKey::generate();
        let public = private.public_key();
        let json = serde_json::to_string(&public).expect("serialize failed");
        let deserialized: PublicKey = serde_json::from_str(&json).expect("deserialize failed");
        assert_eq!(public, deserialized);
    }

    #[test]
    fn keypair_generate() {
        let keypair = KeyPair::generate();
        assert_eq!(keypair.public_key().as_bytes().len(), KEY_SIZE);
        assert_eq!(keypair.private_key().as_bytes().len(), KEY_SIZE);
    }

    #[test]
    fn keypair_from_private_key() {
        let private = PrivateKey::generate();
        let expected_public = private.public_key();
        let keypair = KeyPair::from_private_key(private);
        assert_eq!(keypair.public_key(), &expected_public);
    }

    #[test]
    fn generate_keypair_fn() {
        let (private, public) = generate_keypair();
        assert_eq!(private.public_key(), public);
    }

    #[test]
    fn invalid_key_length_rejected() {
        let short_bytes = [0u8; 16];
        assert!(PrivateKey::from_bytes(&short_bytes).is_err());
        assert!(PublicKey::from_bytes(&short_bytes).is_err());
    }

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn public_key_from_bytes_roundtrip(bytes in prop::array::uniform32(any::<u8>())) {
                let public = PublicKey::from_bytes_array(bytes);
                prop_assert_eq!(*public.as_bytes(), bytes);
            }

            #[test]
            fn public_key_base64_roundtrip_prop(bytes in prop::array::uniform32(any::<u8>())) {
                let public = PublicKey::from_bytes_array(bytes);
                let encoded = public.to_base64();
                let decoded = PublicKey::from_base64(&encoded);
                prop_assert!(decoded.is_ok());
                prop_assert_eq!(public, decoded.unwrap());
            }

            #[test]
            fn keypair_derivation_consistent(seed: [u8; 32]) {
                let private = PrivateKey::from_bytes_array(seed);
                let public1 = private.public_key();
                let public2 = private.public_key();
                prop_assert_eq!(public1, public2);
            }
        }
    }
}
