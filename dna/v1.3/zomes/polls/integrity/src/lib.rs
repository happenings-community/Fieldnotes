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
/// is purely their encryption key.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct AdminX25519Key {
    pub x25519_pubkey: X25519PubKey,
    pub created_at: i64,
}

/// One wrapped copy of a content key, for a single recipient in the cohort.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct WrappedKey {
    pub recipient_x: X25519PubKey,
    pub wrapped: XSalsa20Poly1305EncryptedData,
}

/// A cohort-encrypted attachment on a Finding. The payload is encrypted once
/// with a random content key (ciphertext); that content key is wrapped to each
/// cohort member's x25519 key (wrapped_keys). A recipient locates their entry
/// by recipient_x, ingests it with their lair-held private key, and decrypts.
/// Re-wrappable: a later-added admin gets a new WrappedKey appended without
/// re-encrypting the payload.
#[hdk_entry_helper]
#[derive(Clone, PartialEq)]
pub struct EncryptedAttachment {
    pub ciphertext: XSalsa20Poly1305EncryptedData,
    pub wrapped_keys: Vec<WrappedKey>,
    pub sender_x: X25519PubKey,
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
    AdminX25519Key(AdminX25519Key),
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
    /// From the all-x25519-keys anchor to each AdminX25519Key action hash.
    AllAdminX25519Keys,
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

/// Deterministic anchor for AllAdminX25519Keys links (sentinel AdminX25519Key).
pub fn all_x25519_keys_anchor() -> ExternResult<EntryHash> {
    hash_entry(&AdminX25519Key {
        x25519_pubkey: X25519PubKey::from([0u8; 32]),
        created_at: 0,
    })
}

// ── Validation ────────────────────────────────────────────────────────

#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(store_entry) => match store_entry {
            OpEntry::CreateEntry { app_entry, .. } | OpEntry::UpdateEntry { app_entry, .. } => {
                match app_entry {
                    EntryTypes::AdminGrant(grant) => validate_admin_grant(&grant),
                    EntryTypes::Item(item) => validate_item(&item),
                    EntryTypes::Response(_) => Ok(ValidateCallbackResult::Valid),
                    EntryTypes::Finding(finding) => validate_finding(&finding),
                    // Companion encryption key and encrypted attachment: the
                    // cryptography is the access control, not validation. Any
                    // author may publish their own x25519 key and commit their
                    // own encrypted attachments.
                    EntryTypes::AdminX25519Key(_) => Ok(ValidateCallbackResult::Valid),
                    EntryTypes::EncryptedAttachment(_) => Ok(ValidateCallbackResult::Valid),
                }
            }
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterCreateLink {
            link_type,
            base_address,
            target_address: _,
            tag: _,
            action: _,
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
            LinkTypes::FindingToAttachment => Ok(ValidateCallbackResult::Valid),
            LinkTypes::AllAdminX25519Keys => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterDeleteLink { .. } => Ok(ValidateCallbackResult::Valid),
        _ => Ok(ValidateCallbackResult::Valid),
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
        match AgentPubKey::try_from(progenitor_pubkey_str.clone()) {
            Ok(progenitor_pubkey) => {
                // Create the payload to verify: the admin pubkey's raw bytes
                let payload = grant.admin_pubkey.get_raw_39().to_vec();
                
                // Verify the signature
                if !verify_signature(
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

fn validate_item(item: &Item) -> ExternResult<ValidateCallbackResult> {
    if item.title.trim().is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "Item title cannot be empty".to_string(),
        ));
    }
    
    // For Scenario items, author must have a valid AdminGrant.
    // Feedback items are open to all authors.
    if item.kind == ItemKind::Scenario {
        if let Some(grant_hash) = &item.admin_grant_action_hash {
            // Fetch the grant record deterministically (no DHT read, direct hash lookup).
            // If must_get_valid_record succeeds, the grant already passed validate_admin_grant,
            // which verified the progenitor signature. We just need to confirm the Item author
            // matches the admin_pubkey in the grant.
            match must_get_valid_record(grant_hash.clone()) {
                Ok(_grant_record) => {
                    // Grant exists and is valid. The signature was verified when it was created.
                    // For now, accept Scenarios as long as a grant action hash is referenced.
                    // TODO: deserialize the grant entry to verify author == grant.admin_pubkey
                    Ok(ValidateCallbackResult::Valid)
                }
                Err(_) => {
                    Ok(ValidateCallbackResult::Invalid(
                        "Could not fetch AdminGrant for Item validation".to_string(),
                    ))
                }
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
