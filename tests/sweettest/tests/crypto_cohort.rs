//! Cohort encryption round-trip + RE-WRAP, proven in Fieldnotes.
//!
//! Adapted from R&O's proven crypto spike (7f6aa81c). Three agents: Alice
//! (uploader/handler), Bob (admin cohort member), Carol (a LATER-added admin).
//! Proves: a per-case content key encrypts the payload once; it can be wrapped
//! to a cohort member who decrypts; and the SAME key can be re-wrapped to a
//! later-admitted admin who also decrypts — without re-encrypting the payload.
//! This is the exact property cohort-scoped private attachments need.
//!
//! Calls Fieldnotes' ported crypto functions in the `polls` coordinator zome.

use holochain::prelude::*;
use fieldnotes_sweettest::common::*;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct CryptoCreateOutput {
    key_ref: XSalsa20Poly1305KeyRef,
    ciphertext: XSalsa20Poly1305EncryptedData,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct CryptoWrapInput {
    sender_x: X25519PubKey,
    recipient_x: X25519PubKey,
    key_ref: XSalsa20Poly1305KeyRef,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct CryptoOpenInput {
    recipient_x: X25519PubKey,
    sender_x: X25519PubKey,
    wrapped_key: XSalsa20Poly1305EncryptedData,
    ciphertext: XSalsa20Poly1305EncryptedData,
}

#[tokio::test(flavor = "multi_thread")]
async fn cohort_crypto_roundtrip_and_rewrap() {
    let (conductors, alice, bob, carol) = setup_three_agents().await;

    // 1. Each cohort member mints an x25519 encryption key.
    let alice_x: X25519PubKey = conductors[0]
        .call(&alice.zome("polls"), "crypto_new_x25519", ())
        .await;
    let bob_x: X25519PubKey = conductors[1]
        .call(&bob.zome("polls"), "crypto_new_x25519", ())
        .await;
    let carol_x: X25519PubKey = conductors[2]
        .call(&carol.zome("polls"), "crypto_new_x25519", ())
        .await;

    let secret = b"admin cohort eyes only".to_vec();

    // 2. Uploader mints the per-case content key and encrypts the payload once.
    let case: CryptoCreateOutput = conductors[0]
        .call(&alice.zome("polls"), "crypto_create_encrypted", secret.clone())
        .await;

    // 3. Wrap the content key to Bob (cohort member); Bob opens and decrypts.
    let wrap_bob: XSalsa20Poly1305EncryptedData = conductors[0]
        .call(
            &alice.zome("polls"),
            "crypto_wrap_key",
            CryptoWrapInput {
                sender_x: alice_x.clone(),
                recipient_x: bob_x.clone(),
                key_ref: case.key_ref.clone(),
            },
        )
        .await;
    let bob_plain: Vec<u8> = conductors[1]
        .call(
            &bob.zome("polls"),
            "crypto_open_encrypted",
            CryptoOpenInput {
                recipient_x: bob_x.clone(),
                sender_x: alice_x.clone(),
                wrapped_key: wrap_bob,
                ciphertext: case.ciphertext.clone(),
            },
        )
        .await;
    assert_eq!(bob_plain, secret, "cohort member (Bob) should decrypt the payload");

    // 4. RE-WRAP the same content key to Carol (a LATER-added admin); she opens it.
    let wrap_carol: XSalsa20Poly1305EncryptedData = conductors[0]
        .call(
            &alice.zome("polls"),
            "crypto_wrap_key",
            CryptoWrapInput {
                sender_x: alice_x.clone(),
                recipient_x: carol_x.clone(),
                key_ref: case.key_ref.clone(),
            },
        )
        .await;
    let carol_plain: Vec<u8> = conductors[2]
        .call(
            &carol.zome("polls"),
            "crypto_open_encrypted",
            CryptoOpenInput {
                recipient_x: carol_x.clone(),
                sender_x: alice_x.clone(),
                wrapped_key: wrap_carol,
                ciphertext: case.ciphertext.clone(),
            },
        )
        .await;
    assert_eq!(carol_plain, secret, "later-added admin (Carol) should decrypt via re-wrap");
}
