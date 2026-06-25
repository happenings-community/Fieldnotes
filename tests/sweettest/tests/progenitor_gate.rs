//! Path C cross-peer enforcement, proven across two real agents.
//!
//! Alice is the network's progenitor (her pubkey is burned into the DNA
//! properties). Bob is an ordinary member on the same network. Proves:
//!   1. Alice (progenitor) can self-grant admin — the grant validates.
//!   2. Bob (non-progenitor) CANNOT self-grant — validation rejects it.
//!   3. A Scenario (Item) Alice creates syncs to Bob over the DHT.

use holochain::prelude::*;
use fieldnotes_sweettest::common::*;
use holochain::sweettest::await_consistency;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct AddAdministratorInput {
    admin_pubkey: AgentPubKey,
    progenitor_signature: Signature,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
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
struct StoreAttachmentInput {
    finding_action_hash: ActionHash,
    image_ciphertext: Vec<u8>,
    bulk_nonce: Vec<u8>,
    per_recipient: Vec<RecipientWrappedKey>,
    sender_ed25519: Vec<u8>,
    media_hint: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct RecipientWrappedKey {
    recipient_ed25519: Vec<u8>,
    nonce: Vec<u8>,
    wrapped_key: Vec<u8>,
}

#[tokio::test(flavor = "multi_thread")]
async fn progenitor_gate_across_two_agents() {
    let (conductors, alice, bob, alice_key) = setup_two_agents_with_progenitor().await;

    // 1. Alice (progenitor) self-grants: sign her pubkey's raw 39 bytes with
    //    her own key (zome verifies with verify_signature_raw vs progenitor).
    let alice_payload = alice_key.get_raw_39().to_vec();
    let alice_sig: Signature = conductors[0]
        .keystore()
        .sign(alice_key.clone(), alice_payload.into())
        .await
        .expect("Alice signs");

    let alice_grant: ActionHash = conductors[0]
        .call(
            &alice.zome("polls"),
            "add_administrator",
            AddAdministratorInput {
                admin_pubkey: alice_key.clone(),
                progenitor_signature: alice_sig,
            },
        )
        .await;
    println!("Alice admin grant committed: {:?}", alice_grant);

    // 2. Bob (non-progenitor) attempts to self-grant — must be rejected.
    let bob_key = bob.agent_pubkey().clone();
    let bob_payload = bob_key.get_raw_39().to_vec();
    let bob_sig: Signature = conductors[1]
        .keystore()
        .sign(bob_key.clone(), bob_payload.into())
        .await
        .expect("Bob signs");

    let bob_result: Result<ActionHash, _> = conductors[1]
        .call_fallible(
            &bob.zome("polls"),
            "add_administrator",
            AddAdministratorInput {
                admin_pubkey: bob_key.clone(),
                progenitor_signature: bob_sig,
            },
        )
        .await;
    assert!(
        bob_result.is_err(),
        "Bob (non-progenitor) must NOT self-grant admin, but it succeeded"
    );
    println!("Bob's self-grant correctly rejected: {:?}", bob_result.err());

    // 3. Alice creates a Scenario; it must sync to Bob.
    let item_hash: ActionHash = conductors[0]
        .call(
            &alice.zome("polls"),
            "create_item",
            CreateItemInput {
                kind: ItemKind::Scenario,
                admin_grant_action_hash: Some(alice_grant.clone()),
                campaign: "Cross-peer test".to_string(),
                section: "Sync".to_string(),
                title: "Cross-peer scenario".to_string(),
                instructions: "Created by Alice, visible to Bob.".to_string(),
                look_for: "Bob sees it".to_string(),
                order: 1,
            },
        )
        .await;
    println!("Alice created item: {:?}", item_hash);

    await_consistency(60, [&alice, &bob])
        .await
        .expect("DHT consistency");

    let bobs_view: Vec<Record> = conductors[1]
        .call(&bob.zome("polls"), "get_all_items", ())
        .await;
    assert!(
        !bobs_view.is_empty(),
        "Bob should see Alice's Scenario, saw none"
    );
    println!("Bob sees {} item(s) — cross-peer sync confirmed.", bobs_view.len());

    // 4. Author-binding: Bob cites ALICE's (valid) grant to create a Scenario.
    //    Alice's grant has synced to Bob by now (await_consistency above), so
    //    the grant is fetchable on Bob's side — but Bob is NOT the admin it
    //    names, so validate_item must reject on the author mismatch.
    let bob_foreign: Result<ActionHash, _> = conductors[1]
        .call_fallible(
            &bob.zome("polls"),
            "create_item",
            CreateItemInput {
                kind: ItemKind::Scenario,
                admin_grant_action_hash: Some(alice_grant.clone()),
                campaign: "Forged".to_string(),
                section: "Author-binding".to_string(),
                title: "Bob's illicit scenario".to_string(),
                instructions: "Should be rejected — Bob is not the grant's admin.".to_string(),
                look_for: "rejection".to_string(),
                order: 99,
            },
        )
        .await;
    assert!(
        bob_foreign.is_err(),
        "Bob citing Alice's grant to create a Scenario must be rejected (author-binding), but it succeeded"
    );
    println!(
        "Bob's foreign-grant Scenario correctly rejected: {:?}",
        bob_foreign.err()
    );

    // 5. Attach-to-own-finding. Alice and Bob each create a finding on the
    //    shared Item (findings are open to all). Bob attaching to ALICE's
    //    finding must be REJECTED (link author Bob != finding author Alice).
    let alice_finding: ActionHash = conductors[0]
        .call(
            &alice.zome("polls"),
            "create_finding",
            CreateFindingInput {
                item_action_hash: item_hash.clone(),
                text: "Alice's finding".to_string(),
            },
        )
        .await;
    let bob_finding: ActionHash = conductors[1]
        .call(
            &bob.zome("polls"),
            "create_finding",
            CreateFindingInput {
                item_action_hash: item_hash.clone(),
                text: "Bob's corroborating finding".to_string(),
            },
        )
        .await;
    await_consistency(60, [&alice, &bob])
        .await
        .expect("DHT consistency (findings)");

    let dummy_attachment = |finding: ActionHash| StoreAttachmentInput {
        finding_action_hash: finding,
        image_ciphertext: vec![1, 2, 3],
        bulk_nonce: vec![0u8; 12],
        per_recipient: vec![],
        sender_ed25519: vec![0u8; 32],
        media_hint: "image/png".to_string(),
    };

    // Bob -> Alice's finding: must be rejected by the FindingToAttachment
    // link validation (author-binding).
    let bob_to_alice: Result<ActionHash, _> = conductors[1]
        .call_fallible(
            &bob.zome("polls"),
            "store_encrypted_attachment",
            dummy_attachment(alice_finding.clone()),
        )
        .await;
    assert!(
        bob_to_alice.is_err(),
        "Bob attaching to Alice's finding must be rejected (attach-to-own-finding), but it succeeded"
    );
    println!(
        "Bob's attachment to Alice's finding correctly rejected: {:?}",
        bob_to_alice.err()
    );

    // 6. Positive: Bob attaching to his OWN finding must SUCCEED — this is the
    //    corroboration-with-evidence path ("me too, here's my screenshot").
    let bob_to_own: ActionHash = conductors[1]
        .call(
            &bob.zome("polls"),
            "store_encrypted_attachment",
            dummy_attachment(bob_finding.clone()),
        )
        .await;
    println!(
        "Bob's attachment to his own finding succeeded: {:?} — corroboration-with-evidence works.",
        bob_to_own
    );
}
