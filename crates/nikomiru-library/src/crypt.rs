// SPDX-License-Identifier: GPL-3.0-only

//! Encrypted sync files (BAK-012): what goes to the WebDAV folder, sealed
//! with a passphrase that never leaves the computers. A sealed file is
//! `MAGIC`, a random 16-byte salt, a random 24-byte nonce, then the
//! XChaCha20-Poly1305 ciphertext with its tag; the key is Argon2id of the
//! passphrase and the salt (64 MiB, 3 passes, one lane), and the magic and
//! salt are authenticated too. A file without the magic is plain, as every
//! file was before 2.172.0.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

/// The start of a sealed file.
pub const MAGIC: &[u8; 8] = b"NKMRENC1";
const SALT: usize = 16;
const NONCE: usize = 24;
const HEADER: usize = MAGIC.len() + SALT + NONCE;
/// Argon2id's memory, in KiB.
const MEMORY_KIB: u32 = 64 * 1024;
const PASSES: u32 = 3;

/// Why a file could not be opened or sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CryptError {
    /// Sealed, and no passphrase is set here.
    #[error("the file is encrypted and no passphrase is set")]
    NoPassphrase,
    /// Sealed with another passphrase, or changed since.
    #[error("the file is encrypted with another passphrase, or damaged")]
    Wrong,
    /// Too short to be a sealed file.
    #[error("the encrypted file is damaged")]
    Damaged,
    /// The system gave no random bytes, or Argon2 refused.
    #[error("cannot encrypt: {0}")]
    Unavailable(&'static str),
}

/// Whether `bytes` were sealed.
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

fn key(passphrase: &str, salt: &[u8]) -> Result<Key, CryptError> {
    let params = Params::new(MEMORY_KIB, PASSES, 1, Some(32))
        .map_err(|_| CryptError::Unavailable("argon2 parameters"))?;
    let mut key = Key::default();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|_| CryptError::Unavailable("argon2"))?;
    Ok(key)
}

/// Seals `plain` with `passphrase`, with a new salt and nonce.
pub fn seal(passphrase: &str, plain: &[u8]) -> Result<Vec<u8>, CryptError> {
    let mut random = [0u8; SALT + NONCE];
    getrandom::fill(&mut random).map_err(|_| CryptError::Unavailable("random"))?;
    let (salt, nonce) = random.split_at(SALT);
    let mut out = Vec::with_capacity(HEADER + plain.len() + 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(salt);
    out.extend_from_slice(nonce);
    let cipher = XChaCha20Poly1305::new(&key(passphrase, salt)?);
    let nonce = XNonce::try_from(nonce).map_err(|_| CryptError::Unavailable("nonce"))?;
    let sealed = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plain,
                aad: &out[..MAGIC.len() + SALT],
            },
        )
        .map_err(|_| CryptError::Unavailable("xchacha20poly1305"))?;
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Opens a sealed file; a plain one is given back as it is. An empty
/// passphrase is none.
pub fn open(passphrase: &str, bytes: &[u8]) -> Result<Vec<u8>, CryptError> {
    if !is_sealed(bytes) {
        return Ok(bytes.to_vec());
    }
    if bytes.len() < HEADER + 16 {
        return Err(CryptError::Damaged);
    }
    if passphrase.is_empty() {
        return Err(CryptError::NoPassphrase);
    }
    let salt = &bytes[MAGIC.len()..MAGIC.len() + SALT];
    let nonce =
        XNonce::try_from(&bytes[MAGIC.len() + SALT..HEADER]).map_err(|_| CryptError::Damaged)?;
    XChaCha20Poly1305::new(&key(passphrase, salt)?)
        .decrypt(
            &nonce,
            Payload {
                msg: &bytes[HEADER..],
                aad: &bytes[..MAGIC.len() + SALT],
            },
        )
        .map_err(|_| CryptError::Wrong)
}

/// Seals with `passphrase`, or leaves plain when it is empty.
pub fn seal_if(passphrase: &str, plain: Vec<u8>) -> Result<Vec<u8>, CryptError> {
    if passphrase.is_empty() {
        Ok(plain)
    } else {
        seal(passphrase, &plain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-BAK-012
    #[test]
    fn a_file_is_sealed_and_opened_with_its_passphrase_only() {
        let plain = "進捗 progress".as_bytes();
        let a = seal("correct horse", plain).unwrap();
        let b = seal("correct horse", plain).unwrap();
        assert!(is_sealed(&a));
        assert_ne!(a, b, "a new salt and nonce each time");
        assert!(
            !a.windows(plain.len()).any(|w| w == plain),
            "nothing in clear"
        );
        assert_eq!(open("correct horse", &a).unwrap(), plain);
        assert_eq!(open("wrong horse", &a), Err(CryptError::Wrong));
        assert_eq!(open("", &a), Err(CryptError::NoPassphrase));
        // the header is authenticated: another salt is another key
        let mut changed = a.clone();
        changed[MAGIC.len()] ^= 1;
        assert_eq!(open("correct horse", &changed), Err(CryptError::Wrong));
        let mut tail = a.clone();
        *tail.last_mut().unwrap() ^= 1;
        assert_eq!(open("correct horse", &tail), Err(CryptError::Wrong));
        assert_eq!(
            open("correct horse", &a[..HEADER + 3]),
            Err(CryptError::Damaged)
        );
        // plain files are read as before, with or without a passphrase
        assert_eq!(open("correct horse", b"{plain}").unwrap(), b"{plain}");
        assert_eq!(open("", b"{plain}").unwrap(), b"{plain}");
        assert_eq!(seal_if("", b"x".to_vec()).unwrap(), b"x");
        assert!(is_sealed(&seal_if("p", b"x".to_vec()).unwrap()));
    }
}
