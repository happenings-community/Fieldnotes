//! Full encrypted-attachment loop, proven end-to-end through the REAL feature
//! functions (not just the low-level primitives).
//!
//! Flow: Alice (uploader) creates a Feedback item + a finding. Bob and Carol
//! (the admin cohort) each publish an x25519 companion key. Alice creates an
//! encrypted attachment on the finding — the zome fetches the published cohort
//! keys, wraps the content key to each, and stores it. Bob and Carol each
//! decrypt via the real decrypt_attachment function using their lair-held key.
//!
//! This proves the published-key path, the anchor fetch, the cohort wrap, and
//! per-recipient decrypt — the actual feature, not a primitive stand-in.

use holochain::prelude::*;
use holochain::sweettest::await_consistency;
use fieldnotes_sweettest::common::*;

// ── Mirrors of the coordinator's input/enum types ──────────────────────

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
enum ItemKind {
    Scenario,
    Feedback,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct CreateItemInput {
    kind: ItemKind,
    admin_grant_action_hash: Option<ActionHash>,
    campaign: String,
    section: String,
    title: String,
    instructions: String,
    look_for: String,
    order: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct CreateFindingInput {
    item_action_hash: ActionHash,
    text: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct CreateEncryptedAttachmentInput {
    finding_action_hash: ActionHash,
    plaintext: Vec<u8>,
    sender_x: X25519PubKey,
    media_hint: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct DecryptAttachmentInput {
    attachment_action_hash: ActionHash,
    recipient_x: X25519PubKey,
}

#[tokio::test(flavor = "multi_thread")]
async fn encrypted_attachment_full_loop() {
    let (conductors, alice, bob, carol) = setup_three_agents().await;

    // 1. Bob and Carol publish their x25519 companion keys (the cohort).
    let _bob_x: X25519PubKey = conductors[1]
        .call(&bob.zome("polls"), "publish_my_x25519_key", ())
        .await;
    let _carol_x: X25519PubKey = conductors[2]
        .call(&carol.zome("polls"), "publish_my_x25519_key", ())
        .await;

    // Wait for the published-key links to reach Alice.
    await_consistency(30, [&alice, &bob, &carol]).await.unwrap();

    // 2. Alice creates a Feedback item (no admin gate) to hang a finding on.
    let item_hash: ActionHash = conductors[0]
        .call(
            &alice.zome("polls"),
            "create_item",
            CreateItemInput {
                kind: ItemKind::Feedback,
                admin_grant_action_hash: None,
                campaign: "test".into(),
                section: "test".into(),
                title: "attachment test".into(),
                instructions: "n/a".into(),
                look_for: "n/a".into(),
                order: 0,
            },
        )
        .await;

    // 3. Alice adds a finding on that item.
    let finding_hash: ActionHash = conductors[0]
        .call(
            &alice.zome("polls"),
            "create_finding",
            CreateFindingInput {
                item_action_hash: item_hash,
                text: "see attached screenshot".into(),
            },
        )
        .await;

    // 4. Alice generates her own sender x25519 key.
    let alice_x: X25519PubKey = conductors[0]
        .call(&alice.zome("polls"), "crypto_new_x25519", ())
        .await;

    // 5. Alice creates the encrypted attachment. The zome fetches the cohort's
    //    published keys, wraps to each (+ Alice), stores, links to the finding.
    let secret = b"PNG-bytes-pretend-screenshot-cohort-eyes-only".to_vec();
    let attachment_hash: ActionHash = conductors[0]
        .call(
            &alice.zome("polls"),
            "create_encrypted_attachment",
            CreateEncryptedAttachmentInput {
                finding_action_hash: finding_hash,
                plaintext: secret.clone(),
                sender_x: alice_x,
                media_hint: "image/png".into(),
            },
        )
        .await;

    // Wait for the attachment entry + link to propagate to the cohort.
    await_consistency(30, [&alice, &bob, &carol]).await.unwrap();

    // 6. Bob decrypts via the real function.
    let bob_plain: Vec<u8> = conductors[1]
        .call(
            &bob.zome("polls"),
            "decrypt_attachment",
            DecryptAttachmentInput {
                attachment_action_hash: attachment_hash.clone(),
                recipient_x: _bob_x,
            },
        )
        .await;
    assert_eq!(bob_plain, secret, "Bob (cohort) should decrypt the attachment");

    // 7. Carol decrypts via the real function (the re-wrap/later-admin path).
    let carol_plain: Vec<u8> = conductors[2]
        .call(
            &carol.zome("polls"),
            "decrypt_attachment",
            DecryptAttachmentInput {
                attachment_action_hash: attachment_hash,
                recipient_x: _carol_x,
            },
        )
        .await;
    assert_eq!(carol_plain, secret, "Carol (cohort) should decrypt the attachment");
}
