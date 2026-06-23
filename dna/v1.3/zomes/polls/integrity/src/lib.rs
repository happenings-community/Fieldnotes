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

// ── Field enums ───────────────────────────────────────────────────────

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

#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EntryTypes {
    Item(Item),
    Response(Response),
    Finding(Finding),
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
}

// ── Anchors ───────────────────────────────────────────────────────────

/// Returns a deterministic hash to use as the base for AllItems links.
/// (Mirrors ProofPoll's sentinel-hash anchor approach — proven on hdk 0.6.0.)
pub fn all_items_anchor() -> ExternResult<EntryHash> {
    hash_entry(&Item {
        kind: ItemKind::Scenario,
        campaign: String::new(),
        section: String::new(),
        title: "ALL_ITEMS_ANCHOR".to_string(),
        instructions: String::new(),
        look_for: String::new(),
        order: 0,
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
                    EntryTypes::Item(item) => validate_item(&item),
                    EntryTypes::Response(_) => Ok(ValidateCallbackResult::Valid),
                    EntryTypes::Finding(finding) => validate_finding(&finding),
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
        },
        FlatOp::RegisterDeleteLink { .. } => Ok(ValidateCallbackResult::Valid),
        _ => Ok(ValidateCallbackResult::Valid),
    }
}

fn validate_item(item: &Item) -> ExternResult<ValidateCallbackResult> {
    if item.title.trim().is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "Item title cannot be empty".to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

fn validate_finding(finding: &Finding) -> ExternResult<ValidateCallbackResult> {
    if finding.text.trim().is_empty() {
        return Ok(ValidateCallbackResult::Invalid(
            "Finding text cannot be empty".to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}
