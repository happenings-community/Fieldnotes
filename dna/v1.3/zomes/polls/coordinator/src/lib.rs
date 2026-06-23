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

// ── Input types ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug)]
pub struct CreateItemInput {
    pub kind: ItemKind,
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
pub struct CreateFindingInput {
    pub item_action_hash: ActionHash,
    pub text: String,
}

// ── Item functions ─────────────────────────────────────────────────────

fn do_create_item(input: CreateItemInput) -> ExternResult<ActionHash> {
    let now = sys_time()?.as_seconds_and_nanos().0;

    let item = Item {
        kind: input.kind,
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
