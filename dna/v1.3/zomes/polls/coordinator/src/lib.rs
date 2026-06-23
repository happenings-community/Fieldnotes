//! Fieldnotes coordinator zome (forked from ProofPoll v1.3).
//!
//! Functions:
//!   - `create_item` / `import_items` — seed scenarios (owner; import = bulk from Markdown ingest)
//!   - `get_item` / `get_all_items`   — read scenarios
//!   - `respond`                      — a tester's verdict (one per agent per item, updatable)
//!   - `get_item_responses`           — read verdicts for an item (visible to the cohort)
//!   - `create_finding`               — add a free-text observation (many per agent)
//!   - `get_item_findings`            — read the findings thread for an item
//!
//! Identity/auth is unchanged from ProofPoll and lives in the agent_linking
//! zome — nothing here calls it. Dedup here is per-agent (v0.0.1); the
//! cross-device linked-agent dedup is a later hardening.

use hdk::prelude::*;
use polls_integrity::*;

#[hdk_dependent_entry_types]
enum EntryZomes {
    Integrity(polls_integrity::EntryTypes),
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

// ── Input types ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateItemInput {
    pub kind: ItemKind,
    /// For Scenario items: action hash of the AdminGrant authorizing creation.
    /// For Feedback items: None.
    pub admin_grant_action_hash: Option<ActionHash>,
    pub campaign: String,
    pub section: String,
    pub title: String,
    pub instructions: String,
    pub look_for: String,
    pub order: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RespondInput {
    pub item_action_hash: ActionHash,
    pub verdict: Verdict,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AddAdministratorInput {
    pub admin_pubkey: AgentPubKey,
    pub progenitor_signature: Signature,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateFindingInput {
    pub item_action_hash: ActionHash,
    pub text: String,
}

// ── Item functions ─────────────────────────────────────────────────────

fn do_create_item(input: CreateItemInput) -> ExternResult<ActionHash> {
    let now = sys_time()?.as_seconds_and_nanos().0;

    let item = Item {
        kind: input.kind,
        admin_grant_action_hash: input.admin_grant_action_hash,
        campaign: input.campaign,
        section: input.section,
        title: input.title,
        instructions: input.instructions,
        look_for: input.look_for,
        order: input.order,
        created_at: now,
    };

    let action_hash = create_entry(&EntryZomes::Integrity(EntryTypes::Item(item)))?;

    let anchor = all_items_anchor()?;
    create_link(anchor, action_hash.clone(), LinkTypes::AllItems, ())?;

    Ok(action_hash)
}

#[hdk_extern]
pub fn create_item(input: CreateItemInput) -> ExternResult<ActionHash> {
    do_create_item(input)
}

/// Bulk-create items — the Markdown scenario document parses into many of
/// these, imported in one call. Returns the number created.
#[hdk_extern]
pub fn import_items(items: Vec<CreateItemInput>) -> ExternResult<u32> {
    let mut count: u32 = 0;
    for input in items {
        do_create_item(input)?;
        count += 1;
    }
    Ok(count)
}

#[hdk_extern]
pub fn get_item(action_hash: ActionHash) -> ExternResult<Option<Record>> {
    get(action_hash, GetOptions::default())
}

#[hdk_extern]
pub fn get_all_items(_: ()) -> ExternResult<Vec<Record>> {
    let anchor = all_items_anchor()?;
    let links = get_links(
        LinkQuery::try_new(anchor, LinkTypes::AllItems)?,
        GetStrategy::default(),
    )?;

    let mut records = Vec::new();
    for link in links {
        let hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("Invalid item link target"))?;
        if let Some(record) = get(hash, GetOptions::default())? {
            records.push(record);
        }
    }

    Ok(records)
}

// ── Response functions ──────────────────────────────────────────────────

/// Record (or change) the current agent's verdict on an item.
///
/// One response per agent per item: any prior response by this agent is
/// deleted and replaced, so re-testing (e.g. pass → fail) is a clean update.
/// Dedup is per-agent for v0.0.1.
#[hdk_extern]
pub fn respond(input: RespondInput) -> ExternResult<ActionHash> {
    // Item must exist.
    get(input.item_action_hash.clone(), GetOptions::default())?
        .ok_or(wasm_error!("Item not found"))?;

    let my_agent = agent_info()?.agent_initial_pubkey;

    // Remove any existing response by this agent (entry + its link).
    let existing = get_links(
        LinkQuery::try_new(input.item_action_hash.clone(), LinkTypes::ItemToResponses)?,
        GetStrategy::default(),
    )?;
    for link in &existing {
        if link.author == my_agent {
            if let Ok(target) = ActionHash::try_from(link.target.clone()) {
                delete_link(link.create_link_hash.clone(), GetOptions::default())?;
                delete_entry(target)?;
            }
        }
    }

    let now = sys_time()?.as_seconds_and_nanos().0;
    let response = Response {
        item_action_hash: input.item_action_hash.clone(),
        verdict: input.verdict,
        created_at: now,
    };

    let response_hash = create_entry(&EntryZomes::Integrity(EntryTypes::Response(response)))?;

    create_link(
        input.item_action_hash,
        response_hash.clone(),
        LinkTypes::ItemToResponses,
        (),
    )?;

    Ok(response_hash)
}

#[hdk_extern]
pub fn get_item_responses(item_action_hash: ActionHash) -> ExternResult<Vec<Record>> {
    let links = get_links(
        LinkQuery::try_new(item_action_hash, LinkTypes::ItemToResponses)?,
        GetStrategy::default(),
    )?;

    let mut records = Vec::new();
    for link in links {
        let hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("Invalid response link target"))?;
        if let Some(record) = get(hash, GetOptions::default())? {
            records.push(record);
        }
    }

    Ok(records)
}

// ── Administrator functions ────────────────────────────────────────

/// Create an AdminGrant entry authorizing an agent to create Scenario items.
/// The progenitor signature must be provided by the host (via Tauri/lair).
#[hdk_extern]
pub fn add_administrator(input: AddAdministratorInput) -> ExternResult<ActionHash> {
    let now = sys_time()?.as_seconds_and_nanos().0;
    let grant = AdminGrant {
        admin_pubkey: input.admin_pubkey.clone(),
        progenitor_signature: input.progenitor_signature,
        created_at: now,
    };
    
    let action_hash = create_entry(&EntryZomes::Integrity(EntryTypes::AdminGrant(grant)))?;
    
    // Link from the admin pubkey to the grant for easy lookup
    create_link(
        input.admin_pubkey.clone(),
        action_hash.clone(),
        LinkTypes::AdminToGrant,
        (),
    )?;
    
    Ok(action_hash)
}

/// Get all administrators (agents with valid AdminGrant entries).
#[hdk_extern]
pub fn get_administrators() -> ExternResult<Vec<AgentPubKey>> {
    // Query all AdminGrant entries and return unique admin pubkeys
    // This is a DHT query — coordinator level, not validate time
    // For now, return empty vec as placeholder until we implement the query
    Ok(Vec::new())
}

/// Check if a specific agent pubkey is an administrator.
#[hdk_extern]
pub fn is_administrator(_admin_pubkey: AgentPubKey) -> ExternResult<bool> {
    // Check if the given pubkey has at least one valid AdminGrant
    // For now, return false as placeholder
    Ok(false)
}

// ── Finding functions ───────────────────────────────────────────────────

/// Add a free-text finding to an item. Many per agent; append-only.
#[hdk_extern]
pub fn create_finding(input: CreateFindingInput) -> ExternResult<ActionHash> {
    get(input.item_action_hash.clone(), GetOptions::default())?
        .ok_or(wasm_error!("Item not found"))?;

    let now = sys_time()?.as_seconds_and_nanos().0;
    let finding = Finding {
        item_action_hash: input.item_action_hash.clone(),
        text: input.text,
        created_at: now,
    };

    let finding_hash = create_entry(&EntryZomes::Integrity(EntryTypes::Finding(finding)))?;

    create_link(
        input.item_action_hash,
        finding_hash.clone(),
        LinkTypes::ItemToFindings,
        (),
    )?;

    Ok(finding_hash)
}

#[hdk_extern]
pub fn get_item_findings(item_action_hash: ActionHash) -> ExternResult<Vec<Record>> {
    let links = get_links(
        LinkQuery::try_new(item_action_hash, LinkTypes::ItemToFindings)?,
        GetStrategy::default(),
    )?;

    let mut records = Vec::new();
    for link in links {
        let hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("Invalid finding link target"))?;
        if let Some(record) = get(hash, GetOptions::default())? {
            records.push(record);
        }
    }

    Ok(records)
}
