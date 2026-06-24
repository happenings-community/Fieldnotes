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
        is_archived: false,
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
    // Follow the update chain so the detail screen reflects the latest state.
    get_latest_record(action_hash)
}

/// Resolve an action hash to the LATEST record in its update chain.
///
/// A plain `get()` returns the record for that specific action — for an
/// original Create, that means the original entry, ignoring any later
/// updates. Items can be updated (archive/unarchive flips `is_archived`),
/// so reads must follow the chain to the newest update. `get_details`
/// returns the update actions on a record; we walk to the most recent and
/// return its record, falling back to the original when there are no updates.
fn get_latest_record(original_hash: ActionHash) -> ExternResult<Option<Record>> {
    let Some(details) = get_details(original_hash.clone(), GetOptions::default())? else {
        return Ok(None);
    };

    let record_details = match details {
        Details::Record(rd) => rd,
        _ => return Err(wasm_error!("Expected record details")),
    };

    // No updates: the original is the latest.
    if record_details.updates.is_empty() {
        return Ok(Some(record_details.record));
    }

    // Pick the update with the newest action timestamp.
    let mut latest = record_details.updates[0].clone();
    for upd in record_details.updates.iter() {
        if upd.action().timestamp() > latest.action().timestamp() {
            latest = upd.clone();
        }
    }

    // The update's SignedActionHashed carries the action hash; fetch its full record.
    let latest_hash = latest.action_address().clone();
    get(latest_hash, GetOptions::default())
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
        // Follow the update chain so archive/unarchive is reflected.
        if let Some(record) = get_latest_record(hash)? {
            if let Ok(item) = Item::try_from(record.clone()) {
                if !item.is_archived {
                    records.push(record);
                }
            }
        }
    }

    Ok(records)
}

/// The inverse of get_all_items: the archived ones. Admin-facing, for the
/// control room's "Archived scenarios" section. Same chain-following read,
/// opposite filter.
#[hdk_extern]
pub fn get_archived_items(_: ()) -> ExternResult<Vec<Record>> {
    let anchor = all_items_anchor()?;
    let links = get_links(
        LinkQuery::try_new(anchor, LinkTypes::AllItems)?,
        GetStrategy::default(),
    )?;

    let mut records = Vec::new();
    for link in links {
        let hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("Invalid item link target"))?;
        if let Some(record) = get_latest_record(hash)? {
            if let Ok(item) = Item::try_from(record.clone()) {
                if item.is_archived {
                    records.push(record);
                }
            }
        }
    }

    Ok(records)
}

/// Flip an item's archived flag (admin-only). Shared by archive/unarchive.
///
/// Crucially, the update chains off the LATEST action in the item's chain,
/// not the original — otherwise archive → unarchive → archive would branch
/// the chain into siblings off the original. Resolving to the latest first
/// keeps it linear: original → archived → unarchived → archived.
fn set_item_archived(original_hash: ActionHash, archived: bool) -> ExternResult<ActionHash> {
    let my_pubkey = agent_info()?.agent_initial_pubkey;
    if !is_administrator(my_pubkey.clone())? {
        return Err(wasm_error!("Only administrators can archive items"));
    }

    // Resolve to the latest record so we update the head of the chain.
    let Some(latest) = get_latest_record(original_hash)? else {
        return Err(wasm_error!("Item not found"));
    };
    let latest_hash = latest.action_address().clone();

    let mut item = Item::try_from(latest)?;
    item.is_archived = archived;

    update_entry(latest_hash, item)
}

#[hdk_extern]
pub fn archive_item(action_hash: ActionHash) -> ExternResult<ActionHash> {
    set_item_archived(action_hash, true)
}

#[hdk_extern]
pub fn unarchive_item(action_hash: ActionHash) -> ExternResult<ActionHash> {
    set_item_archived(action_hash, false)
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
    
    // Link from the all-admins anchor to enumerate all admins network-wide
    let anchor = all_admins_anchor()?;
    create_link(
        anchor,
        action_hash.clone(),
        LinkTypes::AllAdmins,
        (),
    )?;
    
    Ok(action_hash)
}

/// Get all administrators (agents with valid AdminGrant entries).
#[hdk_extern]
pub fn get_administrators() -> ExternResult<Vec<AgentPubKey>> {
    let anchor = all_admins_anchor()?;
    let links = get_links(
        LinkQuery::try_new(anchor, LinkTypes::AllAdmins)?,
        GetStrategy::default(),
    )?;
    
    let mut admins = Vec::new();
    for link in links {
        let grant_hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("Invalid admin grant link target"))?;
        if let Some(record) = get(grant_hash, GetOptions::default())? {
            if let Some(entry) = record.entry().as_option() {
                if let Ok(grant) = AdminGrant::try_from(entry.clone()) {
                    if !admins.contains(&grant.admin_pubkey) {
                        admins.push(grant.admin_pubkey);
                    }
                }
            }
        }
    }
    
    Ok(admins)
}

/// Check if a specific agent pubkey is an administrator.
#[hdk_extern]
pub fn is_administrator(admin_pubkey: AgentPubKey) -> ExternResult<bool> {
    let links = get_links(
        LinkQuery::try_new(admin_pubkey, LinkTypes::AdminToGrant)?,
        GetStrategy::default(),
    )?;
    Ok(!links.is_empty())
}

/// Return the AdminGrant action hash for a given agent, if they hold one.
/// Used by the frontend to attach the grant to Scenario creation.
#[hdk_extern]
pub fn get_admin_grant_hash(admin_pubkey: AgentPubKey) -> ExternResult<Option<ActionHash>> {
    let links = get_links(
        LinkQuery::try_new(admin_pubkey, LinkTypes::AdminToGrant)?,
        GetStrategy::default(),
    )?;
    match links.into_iter().next() {
        Some(link) => {
            let grant_hash = ActionHash::try_from(link.target)
                .map_err(|_| wasm_error!("Invalid admin grant link target"))?;
            Ok(Some(grant_hash))
        }
        None => Ok(None),
    }
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

// ── Cohort encryption core (Model B) ───────────────────────────────────
//
// Ported from R&O's proven crypto spike (7f6aa81c). These functions are the
// cryptographic primitive for cohort-scoped private attachments: a random
// shared-secret content key encrypts the payload once, then that key is
// *wrapped* to each recipient's x25519 key. Any recipient ingests the wrapped
// key and decrypts. Re-wrappable: granting a new recipient later means wrapping
// the existing content key to them — no re-encryption of the payload.
//
// All native HDK primitives (x_salsa20_poly1305_*). This is a FEASIBILITY CORE,
// proven to round-trip in Fieldnotes — NOT yet wired to an attachment surface.
// The Ed25519->x25519 bridge (real agent keys -> wrap keys) and the capture/
// storage/decrypt UI are the v0.3 feature build. Here we prove the crypto runs.

#[hdk_extern]
pub fn crypto_new_x25519(_: ()) -> ExternResult<X25519PubKey> {
    create_x25519_keypair()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CryptoCreateOutput {
    pub key_ref: XSalsa20Poly1305KeyRef,
    pub ciphertext: XSalsa20Poly1305EncryptedData,
}

/// Create a random content key and encrypt the payload with it.
#[hdk_extern]
pub fn crypto_create_encrypted(plaintext: Vec<u8>) -> ExternResult<CryptoCreateOutput> {
    let key_ref = x_salsa20_poly1305_shared_secret_create_random(None)?;
    let ciphertext =
        x_salsa20_poly1305_encrypt(key_ref.clone(), XSalsa20Poly1305Data::from(plaintext))?;
    Ok(CryptoCreateOutput { key_ref, ciphertext })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CryptoWrapInput {
    pub sender_x: X25519PubKey,
    pub recipient_x: X25519PubKey,
    pub key_ref: XSalsa20Poly1305KeyRef,
}

/// Wrap the content key to a recipient's x25519 key (export).
#[hdk_extern]
pub fn crypto_wrap_key(input: CryptoWrapInput) -> ExternResult<XSalsa20Poly1305EncryptedData> {
    x_salsa20_poly1305_shared_secret_export(input.sender_x, input.recipient_x, input.key_ref)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CryptoOpenInput {
    pub recipient_x: X25519PubKey,
    pub sender_x: X25519PubKey,
    pub wrapped_key: XSalsa20Poly1305EncryptedData,
    pub ciphertext: XSalsa20Poly1305EncryptedData,
}

/// Ingest the wrapped content key and decrypt the payload.
#[hdk_extern]
pub fn crypto_open_encrypted(input: CryptoOpenInput) -> ExternResult<Vec<u8>> {
    let key_ref = x_salsa20_poly1305_shared_secret_ingest(
        input.recipient_x,
        input.sender_x,
        input.wrapped_key,
        None,
    )?;
    let data = x_salsa20_poly1305_decrypt(key_ref, input.ciphertext)?
        .ok_or_else(|| wasm_error!("decrypt returned None"))?;
    Ok(data.as_ref().to_vec())
}

// ── Encrypted attachments (cohort-scoped) ──────────────────────────────
//
// Storage and key-publishing. The encryption itself is done by the host
// orchestrating the proven crypto_* functions above (create -> wrap-per-cohort
// -> store), exactly as the passing sweettest does. These functions carry no
// crypto logic — they commit entries and links.

/// Publish this agent's x25519 companion public key so uploaders can wrap to
/// it. Generates a fresh keypair (private held in lair) and commits the public
/// key, linked to the all-x25519-keys anchor. Idempotent in spirit: calling
/// again publishes a new key (the newest is used by get_admin_x25519_keys).
#[hdk_extern]
pub fn publish_my_x25519_key(_: ()) -> ExternResult<X25519PubKey> {
    let x_pub = create_x25519_keypair()?;
    let now = sys_time()?.as_seconds_and_nanos().0;
    let entry = AdminX25519Key {
        x25519_pubkey: x_pub.clone(),
        created_at: now,
    };
    let hash = create_entry(&EntryZomes::Integrity(EntryTypes::AdminX25519Key(entry)))?;
    let anchor = all_x25519_keys_anchor()?;
    create_link(anchor, hash, LinkTypes::AllAdminX25519Keys, ())?;
    Ok(x_pub)
}

/// Return every published x25519 companion key (the cohort an uploader wraps
/// the content key to). Deduped by key bytes.
#[hdk_extern]
pub fn get_admin_x25519_keys(_: ()) -> ExternResult<Vec<X25519PubKey>> {
    let anchor = all_x25519_keys_anchor()?;
    let links = get_links(
        LinkQuery::try_new(anchor, LinkTypes::AllAdminX25519Keys)?,
        GetStrategy::Local,
    )?;
    let mut keys: Vec<X25519PubKey> = Vec::new();
    for link in links {
        let hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("Invalid x25519 key link target"))?;
        if let Some(record) = get(hash, GetOptions::local())? {
            if let Ok(k) = AdminX25519Key::try_from(record) {
                if !keys.contains(&k.x25519_pubkey) {
                    keys.push(k.x25519_pubkey);
                }
            }
        }
    }
    Ok(keys)
}

/// Return the x25519 public key THIS agent published (the one uploaders wrap
/// to, so it's the correct recipient key for decrypt). Looks up the agent's
/// own AdminX25519Key entry via the anchor link it authored. Returns the most
/// recent if the agent published more than once. None if never published.
#[hdk_extern]
pub fn get_my_published_x25519_key(_: ()) -> ExternResult<Option<X25519PubKey>> {
    let me = agent_info()?.agent_initial_pubkey;
    let anchor = all_x25519_keys_anchor()?;
    let links = get_links(
        LinkQuery::try_new(anchor, LinkTypes::AllAdminX25519Keys)?,
        GetStrategy::Local,
    )?;

    // Keep only links this agent authored, newest first by timestamp.
    let mut mine: Vec<(Timestamp, ActionHash)> = Vec::new();
    for link in links {
        if link.author == me {
            if let Ok(hash) = ActionHash::try_from(link.target) {
                mine.push((link.timestamp, hash));
            }
        }
    }
    mine.sort_by(|a, b| b.0.cmp(&a.0)); // newest first

    for (_, hash) in mine {
        if let Some(record) = get(hash, GetOptions::local())? {
            if let Ok(k) = AdminX25519Key::try_from(record) {
                return Ok(Some(k.x25519_pubkey));
            }
        }
    }
    Ok(None)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StoreAttachmentInput {
    pub finding_action_hash: ActionHash,
    pub image_ciphertext: Vec<u8>,
    pub bulk_nonce: Vec<u8>,
    pub per_recipient: Vec<RecipientWrappedKey>,
    pub sender_ed25519: Vec<u8>,
    pub media_hint: String,
}

/// Commit a pre-encrypted attachment (encrypted host-side via lair, once per
/// recipient) and link it to its finding. The zome carries no crypto — it only
/// stores the result the host assembled.
#[hdk_extern]
pub fn store_encrypted_attachment(input: StoreAttachmentInput) -> ExternResult<ActionHash> {
    get(input.finding_action_hash.clone(), GetOptions::local())?
        .ok_or(wasm_error!("Finding not found"))?;
    let now = sys_time()?.as_seconds_and_nanos().0;
    let attachment = EncryptedAttachment {
        image_ciphertext: input.image_ciphertext,
        bulk_nonce: input.bulk_nonce,
        per_recipient: input.per_recipient,
        sender_ed25519: input.sender_ed25519,
        media_hint: input.media_hint,
        created_at: now,
    };
    let hash = create_entry(&EntryZomes::Integrity(EntryTypes::EncryptedAttachment(attachment)))?;
    create_link(
        input.finding_action_hash,
        hash.clone(),
        LinkTypes::FindingToAttachment,
        (),
    )?;
    Ok(hash)
}

/// Fetch all encrypted attachments for a finding (records; the host decrypts
/// via crypto_open_encrypted using the caller's lair-held x25519 key).
#[hdk_extern]
pub fn get_finding_attachments(finding_action_hash: ActionHash) -> ExternResult<Vec<Record>> {
    let links = get_links(
        LinkQuery::try_new(finding_action_hash, LinkTypes::FindingToAttachment)?,
        GetStrategy::Local,
    )?;
    let mut records = Vec::new();
    for link in links {
        let hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("Invalid attachment link target"))?;
        if let Some(record) = get(hash, GetOptions::local())? {
            records.push(record);
        }
    }
    Ok(records)
}

/// Fetch a single attachment record by its action hash (host decrypts it).
#[hdk_extern]
pub fn get_attachment_record(attachment_action_hash: ActionHash) -> ExternResult<Option<Record>> {
    get(attachment_action_hash, GetOptions::local())
}
