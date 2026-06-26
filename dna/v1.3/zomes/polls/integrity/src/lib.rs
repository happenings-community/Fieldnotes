//! Fieldnotes integrity zome (forked from ProofPoll v1.3).
//!
//! Data model for directed test scenarios + peer feedback:
//!   - `Item`     — a scenario (owner-seeded) or feedback post. kind = Scenario | Feedback.
//!   - `Response` — a tester's verdict on a scenario. One per agent per item.
//!   - `Finding`  — a free-text observation on an item. Many per agent per item.
//!
//! Identity/auth (the agent_linking zome + Flowsta Vault) is UNCHANGED from
//! ProofPoll and lives in a separate zome — nothing here touches it.
//!
//! v0.0.1 scope notes:
//!   - Findings are plaintext on the DHT (visible to the cohort). Cohort
//!     encryption is a later layer and reuses ProofPoll's EncryptedEntry pattern.
//!   - Validation is intentionally light, matching ProofPoll's posture.

use hdi::prelude::*;

/// Parse an AgentPubKey from its `uhCAk...` multibase string.
///
/// hdi's bare `AgentPubKey::try_from(String)` does NOT decode the holo-hash
/// base64 form (it failed at runtime with "Could not parse progenitor pubkey").
/// This mirrors the host's parse_agent_pub_key_string: strip the 'u' multibase
/// prefix, base64url-decode (no padding), expect 39 bytes (3 prefix + 32 key +
/// 4 DHT location), then from_raw_39. Used to read progenitor_pubkey from the
/// DNA properties during AdminGrant validation.
fn parse_progenitor_pubkey(s: &str) -> Result<AgentPubKey, String> {
    use base64::Engine;
    let b64 = s.strip_prefix('u').ok_or("progenitor pubkey must start with 'u'")?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(b64)
        .map_err(|e| format!("invalid base64 in progenitor pubkey: {}", e))?;
    if raw.len() != 39 {
        return Err(format!("progenitor pubkey wrong length: {} (expected 39)", raw.len()));
    }
    Ok(AgentPubKey::from_raw_39(raw))
}

// ── DNA Properties ─────────────────────────────────────────────────────

/// DNA-level configuration, burned in at hApp bundle time.
/// For development, progenitor_pubkey can be null; in release builds it's
/// the Flowsta Vault agent pubkey of the admin who signed the initial AdminGrant entries.
#[derive(Serialize, Deserialize, Clone, Debug, SerializedBytes)]
pub struct DnaProperties {
    /// The progenitor's Flowsta Vault agent pubkey (hex-encoded AgentPubKey).
    /// Used in validate() to verify AdminGrant signatures.
    /// Null in dev; must be set before distribution.
    pub progenitor_pubkey: Option<String>,
}

impl Default for DnaProperties {
    fn default() -> Self {
        DnaProperties {
            progenitor_pubkey: None,
        }
    }
}

// ── Field enums ───────────────────────────────────────────────────────

/// Cryptographic grant authorizing an agent to create Scenario items.
/// Signed by the progenitor, stored in the DHT. Validates deterministically
/// by verifying the signature against the progenitor pubkey from DNA properties.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct AdminGrant {
    pub admin_pubkey: AgentPubKey,
    pub progenitor_signature: Signature,
    pub created_at: i64,
}

/// Whether an item is a directed test scenario (owner-seeded) or an
/// emergent feedback post raised by a member.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum ItemKind {
    Scenario,
    Feedback,
}

/// A tester's verdict on a scenario.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum Verdict {
    Pass,
    Fail,
    Partial,
    Skip,
}

// ── Entry types ───────────────────────────────────────────────────────

/// A unit of testing or feedback.
///
/// For v0.0.1 we use `Scenario`: the campaign owner seeds these (typically
/// from a single Markdown document), and testers respond to each one.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    /// For Scenario items: action hash of the AdminGrant that authorized creation.
    /// For Feedback items: None. Validate uses this to verify the author's grant.
    pub admin_grant_action_hash: Option<ActionHash>,
    /// Campaign label, e.g. "R&O v0.4.0".
    pub campaign: String,
    /// Section / group, e.g. "Installation & First Launch".
    pub section: String,
    pub title: String,
    /// What to do (Markdown / newline-joined steps).
    pub instructions: String,
    /// What to look for / expected outcome.
    pub look_for: String,
    /// Ordering within the campaign.
    pub order: u32,
    pub created_at: i64,
    /// Admin-archived items are hidden from get_all_items and the board.
    pub is_archived: bool,
}

/// A tester's verdict on a scenario. One per agent per item: a prior
/// response by the same agent is deleted and replaced (see `respond` in the
/// coordinator), so re-testing is a clean update.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct Response {
    pub item_action_hash: ActionHash,
    pub verdict: Verdict,
    pub created_at: i64,
}

/// A free-text observation on an item. Many per agent per item; append-only.
/// Plaintext on the DHT for v0.0.1.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct Finding {
    pub item_action_hash: ActionHash,
    pub text: String,
    pub created_at: i64,
}

// ── Entry type enum ───────────────────────────────────────────────────

/// An administrator's published x25519 companion public key, used as the
/// wrap target for cohort-encrypted attachments. The matching private key is
/// held in lair (generated via create_x25519_keypair); only the public key is
/// published here. Identity (who this admin is) is their agent/Vault key; this
/// An administrator's published x25519 companion public key. DEAD CODE under
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct RecipientWrappedKey {
    /// The recipient's 32-byte Ed25519 agent key (raw, no prefix/location).
    /// Identifies which admin (or the uploader) this wrapped key is for.
    pub recipient_ed25519: Vec<u8>,
    /// 24-byte nonce from lair's crypto_box (for unwrapping the content key).
    pub nonce: Vec<u8>,
    /// The 32-byte ring content key, wrapped to this recipient via lair
    /// crypto_box. Tiny (well under the 8KB lair frame limit) so it is the
    /// only thing encrypted per-recipient; the image is encrypted once.
    pub wrapped_key: Vec<u8>,
}

/// A cohort-encrypted attachment on a Finding. The image is encrypted ONCE with
/// ring (host-side ChaCha20-Poly1305, no IPC frame limit) under a fresh 32-byte
/// content key. That content key is then wrapped per-recipient via lair's
/// crypto_box -- each wrap is tiny (32 bytes), well under lair's 8KB frame limit.
/// A recipient finds their RecipientWrappedKey by matching their agent key,
/// unwraps the content key with their lair-held key, then ring-decrypts the
/// single image_ciphertext. The cohort is the set of admins at upload time;
/// adding an admin later only requires wrapping the 32-byte key to them -- the
/// image is never re-encrypted.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedAttachment {
    /// The image, ring-encrypted ONCE (ciphertext then 16-byte Poly1305 tag).
    pub image_ciphertext: Vec<u8>,
    /// The 12-byte ring nonce used for image_ciphertext.
    pub bulk_nonce: Vec<u8>,
    /// Per-recipient wrapped copies of the 32-byte content key.
    pub per_recipient: Vec<RecipientWrappedKey>,
    /// The uploader's 32-byte Ed25519 agent key (crypto_box sender for wraps).
    pub sender_ed25519: Vec<u8>,
    /// Optional MIME-ish hint for display (e.g. "image/png"); no content leak.
    pub media_hint: String,
    pub created_at: i64,
}

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EntryTypes {
    AdminGrant(AdminGrant),
    Item(Item),
    Response(Response),
    Finding(Finding),
    EncryptedAttachment(EncryptedAttachment),
}

// ── Link types ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
#[hdk_link_types]
pub enum LinkTypes {
    /// From a well-known anchor hash to each Item's action hash.
    AllItems,
    /// From an Item's action hash to each Response's action hash.
    ItemToResponses,
    /// From an Item's action hash to each Finding's action hash.
    ItemToFindings,
    /// From an admin's pubkey to their AdminGrant action hash.
    AdminToGrant,
    /// From the all-admins anchor to each AdminGrant action hash.
    AllAdmins,
    /// From a Finding's action hash to each EncryptedAttachment action hash.
    FindingToAttachment,
}

// ── Anchors ───────────────────────────────────────────────────────────

/// Returns a deterministic hash to use as the base for AllItems links.
/// (Mirrors ProofPoll's sentinel-hash anchor approach — proven on hdk 0.6.0.)
pub fn all_items_anchor() -> ExternResult<EntryHash> {
    hash_entry(&Item {
        kind: ItemKind::Scenario,
        admin_grant_action_hash: None,
        campaign: String::new(),
        section: String::new(),
        title: "ALL_ITEMS_ANCHOR".to_string(),
        instructions: String::new(),
        look_for: String::new(),
        order: 0,
        created_at: 0,
        is_archived: false,
    })
}

/// Returns a deterministic hash to use as the base for AllAdmins links.
/// Used to enumerate all AdminGrant entries network-wide.
pub fn all_admins_anchor() -> ExternResult<EntryHash> {
    hash_entry(&AdminGrant {
        admin_pubkey: AgentPubKey::from_raw_36(vec![0; 36]),
        progenitor_signature: Signature([0u8; 64]),
        created_at: 0,
    })
}

// ── Validation ────────────────────────────────────────────────────────

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(store_entry) => match store_entry {
            // Both Create and Update carry the action (and thus the author),
            // which validate_item needs to bind a Scenario to the admin named
            // in its referenced AdminGrant. Create and Update are distinct
            // types, so we handle them in separate arms and share the inner
            // entry-type dispatch via validate_entry.
            OpEntry::CreateEntry { app_entry, action } => {
                validate_entry(app_entry, &action.author)
            }
            OpEntry::UpdateEntry { app_entry, action, .. } => {
                validate_entry(app_entry, &action.author)
            }
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterCreateLink {
            link_type,
            base_address,
            target_address: _,
            tag: _,
            action,
        } => match link_type {
            LinkTypes::AllItems => {
                let anchor = all_items_anchor()?;
                if base_address != AnyLinkableHash::from(anchor) {
                    return Ok(ValidateCallbackResult::Invalid(
                        "AllItems link must originate from the items anchor".to_string(),
                    ));
                }
                Ok(ValidateCallbackResult::Valid)
            }
            LinkTypes::ItemToResponses => Ok(ValidateCallbackResult::Valid),
            LinkTypes::ItemToFindings => Ok(ValidateCallbackResult::Valid),
            LinkTypes::AdminToGrant => Ok(ValidateCallbackResult::Valid),
            LinkTypes::AllAdmins => Ok(ValidateCallbackResult::Valid),
            LinkTypes::FindingToAttachment => {
                // A FindingToAttachment link attaches an encrypted attachment to
                // a Finding. Require the link author to be the FINDING's author:
                // you may only attach evidence to your OWN finding. Corroboration
                // ("me too, here is my screenshot") is done by creating your own
                // finding on the shared Item and attaching to that — not by
                // writing into someone else's finding.
                let finding_hash = ActionHash::try_from(base_address.clone()).map_err(|_| {
                    wasm_error!("FindingToAttachment base must be a Finding action hash")
                })?;
                let finding_record = must_get_valid_record(finding_hash)?;
                let finding_author = finding_record.action().author().clone();
                if action.author != finding_author {
                    return Ok(ValidateCallbackResult::Invalid(
                        "An attachment may only be linked to a finding by that finding's author"
                            .to_string(),
                    ));
                }
                Ok(ValidateCallbackResult::Valid)
            }
        },
        FlatOp::RegisterDeleteLink { .. } => Ok(ValidateCallbackResult::Valid),
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

/// Dispatch a stored app entry to its type-specific validator, passing the
/// action author through (needed by validate_item for the Scenario gate).
fn validate_entry(
    app_entry: EntryTypes,
    author: &AgentPubKey,
) -> ExternResult<ValidateCallbackResult> {
    match app_entry {
        EntryTypes::AdminGrant(grant) => validate_admin_grant(&grant),
        EntryTypes::Item(item) => validate_item(&item, author),
        EntryTypes::Response(_) => Ok(ValidateCallbackResult::Valid),
        EntryTypes::Finding(finding) => validate_finding(&finding),
        // Encrypted attachment: the cryptography is the access control, not
        // validation. The attachment ciphertext is opaque to peers; the
        // FindingToAttachment link binds it to its finding's author (above).
        EntryTypes::EncryptedAttachment(_) => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_admin_grant(grant: &AdminGrant) -> ExternResult<ValidateCallbackResult> {
    // Basic structural checks
    if grant.created_at == 0 {
        return Ok(ValidateCallbackResult::Invalid(
            "AdminGrant created_at must be non-zero".to_string(),
        ));
    }
    
    // Get DNA properties to read the progenitor pubkey
    let dna_info = dna_info()?;
    let properties = if let Ok(dna_props) = DnaProperties::try_from(dna_info.modifiers.properties) {
        dna_props
    } else {
        DnaProperties::default()
    };
    
    // If progenitor is set, verify the signature
    if let Some(progenitor_pubkey_str) = properties.progenitor_pubkey {
        match parse_progenitor_pubkey(&progenitor_pubkey_str) {
            Ok(progenitor_pubkey) => {
                // Create the payload to verify: the admin pubkey's raw bytes
                let payload = grant.admin_pubkey.get_raw_39().to_vec();

                // Verify the signature over the LITERAL bytes. Vault's /sign
                // signs the raw bytes as-is, so we must use verify_signature_raw
                // (NOT verify_signature, which serializes the data first — that
                // mismatch is exactly why grants failed to verify).
                if !verify_signature_raw(
                    progenitor_pubkey,
                    grant.progenitor_signature.clone(),
                    payload,
                )? {
                    return Ok(ValidateCallbackResult::Invalid(
                        "AdminGrant progenitor_signature does not verify against progenitor pubkey".to_string(),
                    ));
                }
            }
            Err(_) => {
                return Ok(ValidateCallbackResult::Invalid(
                    "Could not parse progenitor pubkey from DNA properties".to_string(),
                ));
            }
        }
    }
    // If progenitor is null (dev mode), accept the grant
    
    Ok(ValidateCallbackResult::Valid)
}

fn validate_item(
    item: &Item,
    author: &AgentPubKey,
) -> ExternResult<ValidateCallbackResult> {
    if item.title.trim().is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "Item title cannot be empty".to_string(),
        ));
    }

    // For Scenario items, the AUTHOR must be the admin named in a valid
    // AdminGrant. Feedback items are open to all authors.
    if item.kind == ItemKind::Scenario {
        if let Some(grant_hash) = &item.admin_grant_action_hash {
            // must_get_valid_record fetches the grant deterministically; if it
            // succeeds the grant already passed validate_admin_grant (progenitor
            // signature verified). We then bind the Scenario's author to that
            // grant: referencing someone else's grant is not enough.
            match must_get_valid_record(grant_hash.clone()) {
                Ok(grant_record) => {
                    let grant: AdminGrant = grant_record
                        .entry()
                        .to_app_option()
                        .map_err(|e| wasm_error!(e))?
                        .ok_or(wasm_error!(
                            "Referenced AdminGrant record has no entry"
                        ))?;
                    if author != &grant.admin_pubkey {
                        return Ok(ValidateCallbackResult::Invalid(
                            "Scenario author does not match the admin in the referenced AdminGrant"
                                .to_string(),
                        ));
                    }
                    Ok(ValidateCallbackResult::Valid)
                }
                Err(_) => Ok(ValidateCallbackResult::Invalid(
                    "Could not fetch AdminGrant for Item validation".to_string(),
                )),
            }
        } else {
            Ok(ValidateCallbackResult::Invalid(
                "Scenario item must reference an AdminGrant action hash".to_string(),
            ))
        }
    } else {
        Ok(ValidateCallbackResult::Valid)
    }
}

fn validate_finding(finding: &Finding) -> ExternResult<ValidateCallbackResult> {
    if finding.text.trim().is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "Finding text cannot be empty".to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}
