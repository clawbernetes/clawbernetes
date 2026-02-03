//! Cryptographic operations for secrets encryption.
//!
//! This module provides encryption and decryption using ChaCha20-Poly1305 AEAD.
//! It supports:
//! - Master key generation
//! - Secret-specific key derivation
//! - Authenticated encryption with random nonces

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Error, Result};
use crate::types::SecretId;

/// Size of the encryption key in bytes (256 bits).
pub const KEY_SIZE: usize = 32;

/// Size of the nonce in bytes (96 bits).
pub const NONCE_SIZE: usize = 12;

/// Size of the authentication tag in bytes (128 bits).
pub const TAG_SIZE: usize = 16;

/// A secret key for encryption operations.
///
/// The key is securely zeroized when dropped.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecretKey {
    bytes: [u8; KEY_SIZE],
}

impl SecretKey {
    /// Generates a new random secret key.
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0u8; KEY_SIZE];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// Creates a `SecretKey` from raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the byte slice is not exactly 32 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != KEY_SIZE {
            return Err(Error::EncryptionError {
                reason: format!("key must be exactly {KEY_SIZE} bytes, got {}", bytes.len()),
            });
        }

        let mut key_bytes = [0u8; KEY_SIZE];
        key_bytes.copy_from_slice(bytes);
        Ok(Self { bytes: key_bytes })
    }

    /// Returns the key bytes as a slice.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Derives a secret-specific key from this master key and a secret ID.
    ///
    /// Uses BLAKE3 key derivation to create a unique key for each secret.
    #[must_use]
    pub fn derive_for_secret(&self, secret_id: &SecretId) -> Self {
        let context = format!("claw-secrets v1 {}", secret_id.as_str());
        let derived = blake3::derive_key(&context, &self.bytes);
        Self { bytes: derived }
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretKey")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

/// Encrypts plaintext using ChaCha20-Poly1305.
///
/// The output format is: `nonce || ciphertext || tag`
///
/// # Errors
///
/// Returns an error if encryption fails.
pub fn encrypt(key: &SecretKey, plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(&key.bytes).map_err(|e| Error::EncryptionError {
        reason: format!("failed to create cipher: {e}"),
    })?;

    // Generate a random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt with AEAD
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| Error::EncryptionError {
            reason: format!("encryption failed: {e}"),
        })?;

    // Prepend nonce to ciphertext
    let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

/// Decrypts ciphertext that was encrypted with [`encrypt`].
///
/// Expects the input format: `nonce || ciphertext || tag`
///
/// # Errors
///
/// Returns an error if:
/// - The ciphertext is too short
/// - Authentication fails (wrong key or tampered data)
pub fn decrypt(key: &SecretKey, ciphertext: &[u8]) -> Result<Vec<u8>> {
    if ciphertext.len() < NONCE_SIZE + TAG_SIZE {
        return Err(Error::EncryptionError {
            reason: format!(
                "ciphertext too short: expected at least {} bytes, got {}",
                NONCE_SIZE + TAG_SIZE,
                ciphertext.len()
            ),
        });
    }

    let cipher = ChaCha20Poly1305::new_from_slice(&key.bytes).map_err(|e| Error::EncryptionError {
        reason: format!("failed to create cipher: {e}"),
    })?;

    // Extract nonce and ciphertext
    let nonce = Nonce::from_slice(&ciphertext[..NONCE_SIZE]);
    let encrypted = &ciphertext[NONCE_SIZE..];

    // Decrypt with authentication
    cipher
        .decrypt(nonce, encrypted)
        .map_err(|e| Error::EncryptionError {
            reason: format!("decryption failed: {e}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_key_generate() {
        let key1 = SecretKey::generate();
        let key2 = SecretKey::generate();

        // Keys should be different (with overwhelming probability)
        assert_ne!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn secret_key_from_bytes_valid() {
        let bytes = [42u8; KEY_SIZE];
        let key = SecretKey::from_bytes(&bytes).expect("should create key");
        assert_eq!(key.as_bytes(), &bytes);
    }

    #[test]
    fn secret_key_from_bytes_wrong_length() {
        let short_bytes = [0u8; 16];
        let result = SecretKey::from_bytes(&short_bytes);
        assert!(result.is_err());

        let long_bytes = [0u8; 64];
        let result = SecretKey::from_bytes(&long_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn secret_key_debug_redacts() {
        let key = SecretKey::generate();
        let debug_str = format!("{key:?}");
        assert!(debug_str.contains("[REDACTED]"));
    }

    #[test]
    fn secret_key_derive_for_secret() {
        let master = SecretKey::generate();
        let id1 = SecretId::new("secret-1").expect("valid id");
        let id2 = SecretId::new("secret-2").expect("valid id");

        let derived1 = master.derive_for_secret(&id1);
        let derived2 = master.derive_for_secret(&id2);

        // Same secret ID should give same derived key
        let derived1_again = master.derive_for_secret(&id1);
        assert_eq!(derived1.as_bytes(), derived1_again.as_bytes());

        // Different secret IDs should give different keys
        assert_ne!(derived1.as_bytes(), derived2.as_bytes());

        // Derived keys should be different from master
        assert_ne!(derived1.as_bytes(), master.as_bytes());
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = SecretKey::generate();
        let plaintext = b"hello, world!";

        let ciphertext = encrypt(&key, plaintext).expect("encryption should succeed");
        let decrypted = decrypt(&key, &ciphertext).expect("decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_produces_different_ciphertexts() {
        let key = SecretKey::generate();
        let plaintext = b"same message";

        let ct1 = encrypt(&key, plaintext).expect("encrypt 1");
        let ct2 = encrypt(&key, plaintext).expect("encrypt 2");

        // Different nonces should produce different ciphertexts
        assert_ne!(ct1, ct2);

        // But both should decrypt to the same plaintext
        let pt1 = decrypt(&key, &ct1).expect("decrypt 1");
        let pt2 = decrypt(&key, &ct2).expect("decrypt 2");
        assert_eq!(pt1, pt2);
        assert_eq!(pt1, plaintext);
    }

    #[test]
    fn decrypt_wrong_key_fails() {
        let key1 = SecretKey::generate();
        let key2 = SecretKey::generate();
        let plaintext = b"secret message";

        let ciphertext = encrypt(&key1, plaintext).expect("encrypt");
        let result = decrypt(&key2, &ciphertext);

        assert!(result.is_err());
    }

    #[test]
    fn decrypt_tampered_data_fails() {
        let key = SecretKey::generate();
        let plaintext = b"secret message";

        let mut ciphertext = encrypt(&key, plaintext).expect("encrypt");

        // Tamper with the ciphertext
        if let Some(byte) = ciphertext.last_mut() {
            *byte ^= 0xFF;
        }

        let result = decrypt(&key, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_too_short_fails() {
        let key = SecretKey::generate();

        // Too short to contain nonce + tag
        let short_data = vec![0u8; NONCE_SIZE + TAG_SIZE - 1];
        let result = decrypt(&key, &short_data);

        assert!(result.is_err());
    }

    #[test]
    fn encrypt_empty_plaintext() {
        let key = SecretKey::generate();
        let plaintext = b"";

        let ciphertext = encrypt(&key, plaintext).expect("encrypt empty");
        let decrypted = decrypt(&key, &ciphertext).expect("decrypt empty");

        assert!(decrypted.is_empty());
    }

    #[test]
    fn encrypt_large_plaintext() {
        let key = SecretKey::generate();
        let plaintext = vec![0xABu8; 1024 * 1024]; // 1MB

        let ciphertext = encrypt(&key, &plaintext).expect("encrypt large");
        let decrypted = decrypt(&key, &ciphertext).expect("decrypt large");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn ciphertext_length_is_predictable() {
        let key = SecretKey::generate();
        let plaintext = b"test message";

        let ciphertext = encrypt(&key, plaintext).expect("encrypt");

        // Output should be: nonce (12) + plaintext (12) + tag (16) = 40
        assert_eq!(ciphertext.len(), NONCE_SIZE + plaintext.len() + TAG_SIZE);
    }
}
