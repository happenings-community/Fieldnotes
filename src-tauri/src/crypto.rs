//! Self-encryption using lair's crypto_box with Ed25519 signing keys.
//!
//! Uses `crypto_box_xsalsa_by_sign_pub_key` for encryption and
//! `crypto_box_xsalsa_open_by_sign_pub_key` for decryption.
//! Lair internally converts Ed25519 → X25519, so we pass the agent's
//! signing key directly. For self-encryption, the agent is both sender
//! and recipient.

use lair_keystore_api::prelude::*;
use std::sync::Arc;

use ring::aead;
use ring::rand::{SecureRandom, SystemRandom};

/// Encrypt data to self using the agent's Ed25519 signing key.
/// Returns (24-byte nonce, ciphertext).
pub async fn encrypt_to_self(
    lair_client: &LairClient,
    agent_ed25519_bytes: [u8; 32],
    plaintext: &[u8],
) -> Result<([u8; 24], Vec<u8>), String> {
    let pub_key: Ed25519PubKey = BinDataSized(Arc::new(agent_ed25519_bytes));
    let data: Arc<[u8]> = plaintext.into();

    let (nonce, cipher) = lair_client
        .crypto_box_xsalsa_by_sign_pub_key(
            pub_key.clone(), // sender = self
            pub_key,         // recipient = self
            None,            // no deep lock passphrase
            data,
        )
        .await
        .map_err(|e| format!("Encryption failed: {}", e))?;

    Ok((nonce, cipher.to_vec()))
}

/// Decrypt data from self using the agent's Ed25519 signing key.
pub async fn decrypt_from_self(
    lair_client: &LairClient,
    agent_ed25519_bytes: [u8; 32],
    nonce: [u8; 24],
    cipher: &[u8],
) -> Result<Vec<u8>, String> {
    let pub_key: Ed25519PubKey = BinDataSized(Arc::new(agent_ed25519_bytes));
    let cipher_arc: Arc<[u8]> = cipher.into();

    let plaintext = lair_client
        .crypto_box_xsalsa_open_by_sign_pub_key(
            pub_key.clone(), // sender = self
            pub_key,         // recipient = self
            None,            // no deep lock passphrase
            nonce,
            cipher_arc,
        )
        .await
        .map_err(|e| format!("Decryption failed: {}", e))?;

    Ok(plaintext.to_vec())
}

/// Encrypt a SMALL payload TO ANOTHER AGENT (the recipient) using both agents'
/// Ed25519 signing keys. Generalises encrypt_to_self: the sender is this agent,
/// the recipient is someone else (e.g. an admin in the cohort). Lair converts
/// both Ed25519 keys to X25519 internally.
///
/// SIZE LIMIT — IMPORTANT. lair's crypto_box routes through lair's IPC, which
/// caps any single call at 8 KB (it raises `FrameOverflow` above 8192 bytes).
/// An earlier version of this docstring wrongly claimed "no size limit ...
/// handles arbitrarily large payloads such as images" — that was disproven in
/// testing and is the exact bug that motivated the ring-hybrid design. Use this
/// ONLY for small payloads, such as the 32-byte content key. For images and
/// other bulk data, encrypt the data ONCE with `bulk_encrypt` (ring, host-side,
/// no IPC frame limit) and wrap only the resulting 32-byte content key to each
/// recipient through here.
/// Returns (24-byte nonce, ciphertext).
pub async fn encrypt_to_agent(
    lair_client: &LairClient,
    sender_ed25519_bytes: [u8; 32],
    recipient_ed25519_bytes: [u8; 32],
    plaintext: &[u8],
) -> Result<([u8; 24], Vec<u8>), String> {
    let sender: Ed25519PubKey = BinDataSized(Arc::new(sender_ed25519_bytes));
    let recipient: Ed25519PubKey = BinDataSized(Arc::new(recipient_ed25519_bytes));
    let data: Arc<[u8]> = plaintext.into();

    let (nonce, cipher) = lair_client
        .crypto_box_xsalsa_by_sign_pub_key(sender, recipient, None, data)
        .await
        .map_err(|e| format!("Encryption to agent failed: {}", e))?;

    Ok((nonce, cipher.to_vec()))
}

/// Decrypt data sent to THIS agent (the recipient) by another agent (the
/// sender), using both Ed25519 signing keys. The recipient must be this agent
/// (its private key is in lair). Mirror of encrypt_to_agent.
pub async fn decrypt_as_recipient(
    lair_client: &LairClient,
    sender_ed25519_bytes: [u8; 32],
    recipient_ed25519_bytes: [u8; 32],
    nonce: [u8; 24],
    cipher: &[u8],
) -> Result<Vec<u8>, String> {
    let sender: Ed25519PubKey = BinDataSized(Arc::new(sender_ed25519_bytes));
    let recipient: Ed25519PubKey = BinDataSized(Arc::new(recipient_ed25519_bytes));
    let cipher_arc: Arc<[u8]> = cipher.into();

    let plaintext = lair_client
        .crypto_box_xsalsa_open_by_sign_pub_key(
            sender, recipient, None, nonce, cipher_arc,
        )
        .await
        .map_err(|e| format!("Decryption as recipient failed: {}", e))?;

    Ok(plaintext.to_vec())
}

/// Sign arbitrary data with the agent's Ed25519 signing key.
/// Returns the signature bytes.
pub async fn sign_raw(
    lair_client: &LairClient,
    agent_ed25519_bytes: [u8; 32],
    data: &[u8],
) -> Result<Vec<u8>, String> {
    let pub_key: Ed25519PubKey = BinDataSized(Arc::new(agent_ed25519_bytes));
    let data_arc: Arc<[u8]> = data.into();

    let signature = lair_client
        .sign_by_pub_key(pub_key, None, data_arc)
        .await
        .map_err(|e| format!("Signing failed: {}", e))?;

    Ok(signature.to_vec())
}

// ---------------------------------------------------------------------------
// Host-side bulk AEAD (ring) — for payloads too large for lair's crypto_box.
//
// lair's crypto_box (above) routes through lair's IPC and rejects any single
// call over 8 KB (FrameOverflow). A screenshot is ~16x that. So the image is
// encrypted ONCE here with ring (a pure-Rust AEAD already in our dependency
// tree via holochain/lair; it does NOT touch lair's IPC, so no 8 KB limit),
// and only the tiny 32-byte content key is wrapped to each recipient through
// lair's crypto_box (encrypt_to_agent) — comfortably under 8 KB. That hybrid
// is what lets a 129 KB attachment be readable by the whole admin cohort.
//
// Cipher: ChaCha20-Poly1305 (AEAD). Chosen over AES-256-GCM because ChaCha
// does not depend on hardware AES instructions for constant-time safety, so it
// is the safer default across arbitrary hardware. 12-byte nonce, 16-byte
// Poly1305 authentication tag appended to the ciphertext.
//
// On `LessSafeKey` (the alarming name): ring offers two ways to drive an AEAD.
// `SealingKey<NonceSequence>` makes ring own the nonce and refuse to ever
// repeat one — its single guarantee is that you cannot reuse a nonce.
// `LessSafeKey` instead trusts the CALLER to supply a unique nonce each time;
// "less safe" flags exactly and only that responsibility. The cipher and its
// security are byte-for-byte identical between the two.
//
// Reusing a (key, nonce) pair across two encryptions is catastrophic for these
// ciphers — that is the hazard the "safe" API guards against. We design that
// hazard out at the KEY level: every call to bulk_encrypt generates a
// brand-new random 32-byte key, used for exactly one encryption ever, plus a
// fresh random nonce. There is no second encryption under a given key, so no
// nonce can collide. The "less safe" caveat therefore does not apply to this
// use. The round-trip + tamper + wrong-key tests below prove the construction.
// ---------------------------------------------------------------------------

/// Bulk-encrypt arbitrary data (e.g. an image) with a fresh, single-use key.
/// Returns (32-byte content key, 12-byte nonce, ciphertext-with-appended-tag).
/// The content key is what you then wrap to each recipient via encrypt_to_agent;
/// the nonce and ciphertext are stored once on the entry.
pub fn bulk_encrypt(plaintext: &[u8]) -> Result<([u8; 32], [u8; 12], Vec<u8>), String> {
    let rng = SystemRandom::new();

    let mut key_bytes = [0u8; 32];
    rng.fill(&mut key_bytes)
        .map_err(|_| "bulk_encrypt: failed to generate content key".to_string())?;

    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| "bulk_encrypt: failed to generate nonce".to_string())?;

    let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &key_bytes[..])
        .map_err(|_| "bulk_encrypt: failed to build AEAD key".to_string())?;
    let key = aead::LessSafeKey::new(unbound);
    let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);

    // seal_in_place_append_tag mutates the buffer in place and appends the
    // 16-byte tag, so on success in_out is (ciphertext || tag).
    let mut in_out = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut in_out)
        .map_err(|_| "bulk_encrypt: sealing failed".to_string())?;

    Ok((key_bytes, nonce_bytes, in_out))
}

/// Bulk-decrypt data produced by bulk_encrypt. The recipient first recovers the
/// 32-byte content key by unwrapping their per-recipient blob via lair
/// (decrypt_as_recipient), then passes it here with the stored nonce and the
/// ciphertext-with-tag. Returns the original plaintext bytes.
pub fn bulk_decrypt(
    key_bytes: &[u8; 32],
    nonce_bytes: &[u8; 12],
    ciphertext_and_tag: &[u8],
) -> Result<Vec<u8>, String> {
    let unbound = aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &key_bytes[..])
        .map_err(|_| "bulk_decrypt: failed to build AEAD key".to_string())?;
    let key = aead::LessSafeKey::new(unbound);
    let nonce = aead::Nonce::assume_unique_for_key(*nonce_bytes);

    // open_in_place verifies the tag, decrypts in place, and returns the
    // plaintext slice (a prefix of the buffer; the tag is stripped). A wrong
    // key, wrong nonce, or any tampering fails here rather than returning junk.
    let mut in_out = ciphertext_and_tag.to_vec();
    let plaintext = key
        .open_in_place(nonce, aead::Aad::empty(), &mut in_out)
        .map_err(|_| {
            "bulk_decrypt: opening failed (wrong key/nonce or tampered ciphertext)".to_string()
        })?;

    Ok(plaintext.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_roundtrip_129kb() {
        // 129 KB — ~16x over lair's 8 KB FrameOverflow, the reason ring is here.
        // A varied byte pattern (not all-zeros) so the round-trip is meaningful.
        let plaintext: Vec<u8> = (0..129 * 1024).map(|i| (i % 251) as u8).collect();

        let (key, nonce, ciphertext) = bulk_encrypt(&plaintext).expect("encrypt");

        // Ciphertext = plaintext length + 16-byte Poly1305 tag.
        assert_eq!(
            ciphertext.len(),
            plaintext.len() + 16,
            "expected a 16-byte tag appended"
        );
        // Body must not be the plaintext in the clear.
        assert_ne!(
            &ciphertext[..plaintext.len()],
            &plaintext[..],
            "ciphertext must differ from plaintext"
        );

        let recovered = bulk_decrypt(&key, &nonce, &ciphertext).expect("decrypt");
        assert_eq!(recovered, plaintext, "round-trip must return identical bytes");
    }

    #[test]
    fn bulk_decrypt_rejects_tampering() {
        let plaintext = b"the cohort can read this; a tamperer cannot".to_vec();
        let (key, nonce, mut ciphertext) = bulk_encrypt(&plaintext).expect("encrypt");

        // Flip one bit in the ciphertext body. Poly1305 must reject it.
        ciphertext[0] ^= 0x01;
        assert!(
            bulk_decrypt(&key, &nonce, &ciphertext).is_err(),
            "tampered ciphertext must fail authentication"
        );
    }

    #[test]
    fn bulk_decrypt_rejects_wrong_key() {
        let plaintext = b"wrong key must not decrypt".to_vec();
        let (_key, nonce, ciphertext) = bulk_encrypt(&plaintext).expect("encrypt");

        assert!(
            bulk_decrypt(&[0u8; 32], &nonce, &ciphertext).is_err(),
            "wrong key must fail authentication"
        );
    }
}
