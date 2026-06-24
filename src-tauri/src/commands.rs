//! Tauri commands for ProofPoll.
//!
//! This is the bridge between the Qwik frontend and the Holochain conductor.
//! All zome calls go through the Rust backend via AppWebsocket — the frontend
//! uses lightweight Tauri `invoke()` calls and never touches @holochain/client.
//!
//! ## For forking developers
//!
//! This file has three sections:
//!   1. **Entry types + response types** (top) — Mirror your zome's Rust structs.
//!      Change these to match your own data model.
//!   2. **Infrastructure** (middle) — `AppState`, `call_zome`, `friendly_error`,
//!      `decode_entry`. Keep these as-is; they work for any Holochain app.
//!   3. **Tauri commands** (bottom) — One `#[tauri::command]` per frontend action.
//!      Replace the poll/vote/flag commands with your own. The identity-linking
//!      and migration commands are reusable infrastructure.
//!
//! After changing commands, register them in `lib.rs` → `invoke_handler`.

use crate::conductor::{ConductorHandle, ConductorStatus};
use holochain_client::AppWebsocket;
use holochain_types::prelude::{
    ActionHash, AgentPubKey, ExternIO, FunctionName, Record, Signature, ZomeName,
};
use lair_keystore_api::prelude::LairClient;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ── Entry types matching the zome definitions ─────────────────────────
//
// These structs must match the entry types in your coordinator zome.
// Holochain entries are MessagePack-encoded, so Serialize + Deserialize
// are required. The frontend never sees these directly — they're decoded
// in the Tauri commands and returned as response types (below).

/// Used to decode MigratedPoll entries when merging v1.0 and v1.1 poll lists.
/// Mirrors the MigratedPoll entry type in the v1.1 integrity zome.
#[derive(serde::Deserialize)]
struct MigratedPollEntry {
    old_action_hash: ActionHash,
    #[allow(dead_code)]
    new_action_hash: ActionHash,
    #[allow(dead_code)]
    migrated_at: i64,
}

/// Mirrors PollType in the v1.2 integrity zome.
///
/// Must use the same serde representation so `decode_entry` works: rmp_serde
/// encodes unit enum variants as externally-tagged maps `{"Public": nil}`, not
/// as strings. Using a matching enum here round-trips correctly. Tauri then
/// re-serialises to JSON (human-readable) which produces the string `"Public"`
/// that the frontend already expects.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum PollType {
    Anonymous,
    Public,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Poll {
    pub title: String,
    pub description: String,
    pub options: Vec<String>,
    pub created_at: i64,
    pub closes_at: Option<i64>,
    /// None for v1.0/v1.1 polls (no poll_type field — treated as Anonymous).
    #[serde(default)]
    pub poll_type: Option<PollType>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Vote {
    pub poll_action_hash: ActionHash,
    pub option_index: u32,
    /// Only present on v1.2 public poll votes.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Only present on v1.2 public poll votes.
    #[serde(default)]
    pub profile_picture: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CreatePollInput {
    pub title: String,
    pub description: String,
    pub options: Vec<String>,
    pub closes_at: Option<i64>,
    /// Only sent for v1.2. Omit for v1.0/v1.1 migration calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_type: Option<String>,
}

#[allow(dead_code)] // dormant: superseded by Fieldnotes types
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CastVoteInput {
    pub poll_action_hash: ActionHash,
    pub option_index: u32,
    /// Only sent for v1.2 public polls; None otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Only sent for v1.2 public polls; None otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_picture: Option<String>,
}

// ── Frontend response types ───────────────────────────────────────────
//
// These are what the frontend receives via Tauri invoke().
// ActionHashes are converted to strings because TypeScript can't handle
// Holochain's native hash types. The frontend uses these string hashes
// to reference entries in subsequent calls.

#[allow(dead_code)] // dormant: superseded by Fieldnotes types
#[derive(serde::Serialize, Clone)]
pub struct PollListItem {
    pub hash: String,
    pub poll: Poll,
    pub author: String,
    /// Which DHT this poll lives on: "1.0", "1.1", or "1.2".
    /// The frontend passes this back when voting so the vote goes to the correct cell.
    pub dna_version: String,
}

#[allow(dead_code)] // dormant: superseded by Fieldnotes types
#[derive(serde::Serialize)]
pub struct PollDetail {
    pub poll: Poll,
    pub author: String,
    /// Which DHT this poll lives on: "1.0", "1.1", or "1.2".
    pub dna_version: String,
}

#[allow(dead_code)] // dormant: superseded by Fieldnotes types
#[derive(serde::Serialize, Clone)]
pub struct VoteData {
    pub vote: VoteResponse,
    pub author: String,
    /// Display name from v1.2 public poll votes. None for anonymous or pre-v1.2 votes.
    pub display_name: Option<String>,
    /// Profile picture URL from v1.2 public poll votes. None for anonymous or pre-v1.2 votes.
    pub profile_picture: Option<String>,
}

#[derive(serde::Serialize, Clone)]
pub struct VoteResponse {
    pub hash: String,
    pub poll_action_hash: String,
    pub option_index: u32,
}

// ── Fieldnotes types (Item / Response / Finding) ──────────────────────
//
// Host-side mirrors of the polls zome entry/input types. As with PollType
// above, the enums mirror the zome's serde representation so rmp_serde
// round-trips at the zome boundary; Tauri then re-serialises to JSON
// strings for the frontend (e.g. "Scenario", "Pass").

/// Mirrors ItemKind in the integrity zome.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum ItemKind {
    Scenario,
    Feedback,
}

/// Mirrors Verdict in the integrity zome.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum Verdict {
    Pass,
    Fail,
    Partial,
    Skip,
}

/// Mirrors the Item entry in the integrity zome.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Item {
    pub kind: ItemKind,
    pub campaign: String,
    pub section: String,
    pub title: String,
    pub instructions: String,
    pub look_for: String,
    pub order: u32,
    pub created_at: i64,
}

/// Mirrors the Response entry in the integrity zome.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Response {
    pub item_action_hash: ActionHash,
    pub verdict: Verdict,
    pub created_at: i64,
}

/// Mirrors the Finding entry in the integrity zome.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Finding {
    pub item_action_hash: ActionHash,
    pub text: String,
    pub created_at: i64,
}

/// Mirrors CreateItemInput in the coordinator zome (no created_at — the
/// zome timestamps on create). Used for both create_item and import_items.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CreateItemInput {
    pub kind: ItemKind,
    pub campaign: String,
    pub section: String,
    pub title: String,
    pub instructions: String,
    pub look_for: String,
    pub order: u32,
}

/// Mirrors RespondInput in the coordinator zome.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RespondInput {
    pub item_action_hash: ActionHash,
    pub verdict: Verdict,
}

/// Mirrors CreateFindingInput in the coordinator zome.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct CreateFindingInput {
    pub item_action_hash: ActionHash,
    pub text: String,
}

// ── Fieldnotes frontend response types ────────────────────────────────
//
// What the frontend receives. ActionHashes become strings (TypeScript
// can't handle native hash types). `author` is the agent pub key string;
// mapping agent keys to display names is a later, identity-layer job.

#[derive(serde::Serialize, Clone)]
pub struct ItemListItem {
    pub hash: String,
    pub item: Item,
    pub author: String,
}

#[derive(serde::Serialize)]
pub struct ItemDetail {
    pub item: Item,
    pub author: String,
}

#[derive(serde::Serialize, Clone)]
pub struct ResponseData {
    pub hash: String,
    pub item_action_hash: String,
    pub verdict: Verdict,
    pub author: String,
    pub created_at: i64,
}

#[derive(serde::Serialize, Clone)]
pub struct FindingData {
    pub hash: String,
    pub item_action_hash: String,
    pub text: String,
    pub author: String,
    pub created_at: i64,
}

// --- App state ---

pub struct AppState {
    pub data_dir: PathBuf,
    pub conductor_handle: Mutex<Option<ConductorHandle>>,
    pub conductor_status: Mutex<ConductorStatus>,
    pub agent_pub_key: Mutex<Option<String>>,
    /// Active (v1.3) app client for all reads and writes.
    pub app_client: tokio::sync::Mutex<Option<AppWebsocket>>,
    /// v1.2 app client for migration reads (v1.2 → v1.3).
    pub app_client_v1_2: tokio::sync::Mutex<Option<AppWebsocket>>,
    /// v1.1 app client for legacy reads.
    pub app_client_v1_1: tokio::sync::Mutex<Option<AppWebsocket>>,
    /// v1.0 app client for legacy reads.
    pub app_client_v1_0: tokio::sync::Mutex<Option<AppWebsocket>>,
    pub passphrase: Mutex<String>,
    pub lair_client: tokio::sync::Mutex<Option<LairClient>>,
    /// Current migration state (persisted to disk).
    pub migration_state: tokio::sync::Mutex<crate::migration::MigrationState>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let passphrase_path = data_dir.join("lair-passphrase");
        let lair_config_path = data_dir.join("lair").join("lair-keystore-config.yaml");
        let lair_store_path = data_dir.join("lair").join("store_file");
        let conductor_dir = data_dir.join("conductor");

        // These three files are encryption-paired: lair-passphrase derives the
        // crypto key, the config holds matching salts, store_file is encrypted
        // under them. Either ALL three are present (a working install) or ALL
        // three are absent (a true fresh install). Any other combination means
        // an uninstall+reinstall removed some files but not others, leaving
        // orphaned state encrypted under a passphrase or salts that no longer
        // exist — every attempt to start lair from that state crashes with
        // `sqlcipher_page_cipher: hmac check failed`. Detect that mismatch
        // here and wipe everything for a clean restart.
        //
        // Nothing user-recoverable lives in either dir: agent keys are
        // regenerated every install, and user-authored polls/votes come back
        // through the Vault backup restore flow on next sign-in.
        let pp = passphrase_path.exists();
        let cfg = lair_config_path.exists();
        let store = lair_store_path.exists();
        let all_present = pp && cfg && store;
        let all_absent = !pp && !cfg && !store;
        if !all_present && !all_absent {
            log::warn!(
                "Inconsistent lair state on startup (lair-passphrase={}, config={}, store_file={}). Wiping lair + conductor data dirs to recover.",
                pp, cfg, store,
            );
            let _ = std::fs::remove_file(&passphrase_path);
            let _ = std::fs::remove_dir_all(data_dir.join("lair"));
            let _ = std::fs::remove_dir_all(&conductor_dir);
        }

        let passphrase = if passphrase_path.exists() {
            std::fs::read_to_string(&passphrase_path).unwrap_or_else(|_| generate_passphrase())
        } else {
            let p = generate_passphrase();
            if let Err(e) = std::fs::write(&passphrase_path, &p) {
                log::error!("Failed to persist lair passphrase to {:?}: {} — next launch will regenerate and wipe state", passphrase_path, e);
            } else {
                log::info!("Generated new lair passphrase at {:?}", passphrase_path);
            }
            p
        };

        let migration_state = crate::migration::MigrationState::load(&data_dir);

        Self {
            data_dir,
            conductor_handle: Mutex::new(None),
            conductor_status: Mutex::new(ConductorStatus::Stopped),
            agent_pub_key: Mutex::new(None),
            app_client: tokio::sync::Mutex::new(None),
            app_client_v1_2: tokio::sync::Mutex::new(None),
            app_client_v1_1: tokio::sync::Mutex::new(None),
            app_client_v1_0: tokio::sync::Mutex::new(None),
            passphrase: Mutex::new(passphrase),
            lair_client: tokio::sync::Mutex::new(None),
            migration_state: tokio::sync::Mutex::new(migration_state),
        }
    }
}

/// Wipe every encryption-paired file under data_dir, generate a fresh
/// random passphrase, persist it, and update the AppState mutex so any
/// subsequent code path sees the new value. Returns the new passphrase
/// for the caller to use immediately.
///
/// Called from `conductor::start_holochain` when the first attempt fails
/// with a SQLCipher hmac mismatch — the only recoverable response to
/// that error is to drop the orphaned encrypted state and start fresh.
/// Nothing user-recoverable lives in either directory: agent keys are
/// regenerated every install, and user-authored polls/votes come back
/// through the Vault backup restore flow on next sign-in.
pub fn nuke_state_and_regenerate_passphrase(data_dir: &Path, app_state: &AppState) -> String {
    log::warn!(
        "Performing full state reset under {:?} (lair-passphrase + lair/ + conductor/)",
        data_dir,
    );
    let passphrase_path = data_dir.join("lair-passphrase");
    if let Err(e) = std::fs::remove_file(&passphrase_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("remove lair-passphrase: {}", e);
        }
    }
    if let Err(e) = std::fs::remove_dir_all(data_dir.join("lair")) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("remove_dir_all lair: {}", e);
        }
    }
    if let Err(e) = std::fs::remove_dir_all(data_dir.join("conductor")) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("remove_dir_all conductor: {}", e);
        }
    }

    let new_passphrase = generate_passphrase();
    if let Err(e) = std::fs::write(&passphrase_path, &new_passphrase) {
        log::error!(
            "Failed to write regenerated lair-passphrase to {:?}: {}",
            passphrase_path, e,
        );
    }

    *app_state.passphrase.lock().unwrap() = new_passphrase.clone();
    log::info!("State reset complete; new passphrase persisted and in-memory");
    new_passphrase
}

fn generate_passphrase() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
        .collect()
}

// ── Helpers (reusable infrastructure) ──────────────────────────────────
//
// These constants and functions work for any Holochain app.
// Change ROLE_NAME to match your happ.yaml role id.
// Change POLLS_ZOME to match your coordinator zome name.

/// Must match the role `id` in your happ.yaml.
const ROLE_NAME: &str = "proofpoll";
/// Your app's coordinator zome name (from dna.yaml).
const POLLS_ZOME: &str = "polls";
/// Flowsta agent-linking zome — keep as-is for identity integration.
const AGENT_LINKING_ZOME: &str = "agent_linking";

fn decode_entry<T: serde::de::DeserializeOwned>(record: &Record) -> Result<T, String> {
    let entry = record
        .entry()
        .as_option()
        .ok_or("Record has no entry")?;
    let app_bytes = entry
        .as_app_entry()
        .ok_or("Not an app entry")?;
    let sb = app_bytes.as_ref();
    rmp_serde::from_slice(sb.bytes()).map_err(|e| format!("Failed to decode entry: {}", e))
}

/// Call a zome function on the active DNA version.
///
/// This is the core helper that all Tauri commands use. It handles:
///   - Routing to the correct zome via ROLE_NAME
///   - Auto-recovery from CellDisabled errors (re-enables the app and retries)
///   - User-friendly error messages via `friendly_error()`
///
/// For forking: you don't need to change this function — just call it
/// with your zome name and function name from your Tauri commands.
async fn call_zome(
    client: &AppWebsocket,
    zome: &str,
    fn_name: &str,
    payload: ExternIO,
) -> Result<ExternIO, String> {
    use holochain_client::ZomeCallTarget;
    use holochain_types::prelude::RoleName;

    let result = client
        .call_zome(
            ZomeCallTarget::RoleName(RoleName::from(ROLE_NAME)),
            ZomeName::from(zome),
            FunctionName::from(fn_name),
            payload.clone(),
        )
        .await;

    match result {
        Ok(r) => Ok(r),
        Err(e) => {
            let err_str = format!("{}", e);
            // Auto-recover from CellDisabled (e.g. after unclean shutdown)
            if err_str.contains("CellDisabled") {
                log::warn!("CellDisabled detected, attempting auto-recovery...");
                if let Err(re) = try_reenable_app().await {
                    log::error!("Auto-recovery failed: {}", re);
                    return Err(friendly_error(&err_str));
                }
                // Retry the zome call once
                log::info!("Retrying zome call after re-enabling app...");
                client
                    .call_zome(
                        ZomeCallTarget::RoleName(RoleName::from(ROLE_NAME)),
                        ZomeName::from(zome),
                        FunctionName::from(fn_name),
                        payload,
                    )
                    .await
                    .map_err(|e2| friendly_error(&format!("{}", e2)))
            } else {
                Err(friendly_error(&err_str))
            }
        }
    }
}

/// Attempt to re-enable the app via admin websocket.
async fn try_reenable_app() -> Result<(), String> {
    use holochain_client::AdminWebsocket;

    let admin_ws = AdminWebsocket::connect(
        format!("localhost:{}", crate::conductor::ADMIN_WS_PORT),
        Some("proofpoll".to_string()),
    )
    .await
    .map_err(|e| format!("Failed to connect to admin WS for recovery: {}", e))?;

    admin_ws
        .enable_app(crate::dna::ACTIVE_APP_ID.to_string())
        .await
        .map_err(|e| format!("Failed to re-enable app: {}", e))?;

    log::info!("App re-enabled successfully");
    Ok(())
}

/// Translate raw Holochain errors into user-friendly messages.
fn friendly_error(raw: &str) -> String {
    if raw.contains("CellDisabled") {
        "Your data cell was temporarily disabled and could not be recovered automatically. Please restart the app.".into()
    } else if raw.contains("WebsocketError") || raw.contains("ConnectionReset") || raw.contains("Io(") {
        "Lost connection to the Holochain conductor. It may have stopped unexpectedly.".into()
    } else if raw.contains("timeout") || raw.contains("Timeout") {
        "The request timed out. The network may be slow or unreachable.".into()
    } else if raw.contains("Conductor returned an error") {
        // Strip the nested conductor error wrapper for clarity
        if let Some(inner) = raw.split("InternalError(\"").nth(1) {
            let inner = inner.trim_end_matches("\")");
            format!("Conductor error: {}", inner)
        } else {
            format!("Conductor error: {}", raw)
        }
    } else {
        format!("Something went wrong: {}", raw)
    }
}

/// Parse a uhCAk... agent key string into an AgentPubKey.
/// Decodes the base64url body and preserves the exact 39 bytes (including
/// DHT location) so the key matches what the external signer used.
pub(crate) fn parse_agent_pub_key_string(s: &str) -> Result<AgentPubKey, String> {
    // Strip the "u" multibase prefix
    let b64 = s.strip_prefix('u').ok_or("Agent key must start with 'u'")?;

    // Decode base64url (no padding)
    use base64::Engine;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(b64)
        .map_err(|e| format!("Invalid base64 in agent key: {}", e))?;

    // Expect 39 bytes: 3 prefix + 32 key + 4 location
    if raw.len() != 39 {
        return Err(format!("Agent key wrong length: {} (expected 39)", raw.len()));
    }

    Ok(AgentPubKey::from_raw_39(raw))
}

// ── Status command (infrastructure — keep as-is) ──────────────────────

#[derive(serde::Serialize)]
pub struct AppStatus {
    pub ready: bool,
    pub agent_pub_key: Option<String>,
    pub conductor_status: ConductorStatus,
}

#[tauri::command]
pub fn get_app_status(state: tauri::State<'_, std::sync::Arc<AppState>>) -> AppStatus {
    let status = state.conductor_status.lock().unwrap().clone();
    let agent_key = state.agent_pub_key.lock().unwrap().clone();
    let ready = matches!(status, ConductorStatus::Ready { .. });

    log::info!("get_app_status called: ready={}", ready);

    AppStatus {
        ready,
        agent_pub_key: agent_key,
        conductor_status: status,
    }
}

/// Best-effort: open the Flowsta Vault desktop app if it's installed but not
/// running, so the user doesn't have to go find it during Flowsta sign-in.
/// Tries the usual install locations per OS; returns an error string if it
/// can't find/launch it (the UI then just asks the user to open it manually).
#[tauri::command]
pub fn launch_vault() -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Flowsta Vault"])
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open Flowsta Vault: {e}"))
    }

    #[cfg(target_os = "linux")]
    {
        // Packaged installs (.deb/.rpm) put `flowsta-vault` on PATH.
        Command::new("flowsta-vault")
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Could not open Flowsta Vault: {e}"))
    }

    #[cfg(target_os = "windows")]
    {
        // Tauri's NSIS installer puts the app under <install root>/Flowsta Vault.
        for var in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(base) = std::env::var(var) {
                let path = std::path::Path::new(&base)
                    .join("Flowsta Vault")
                    .join("Flowsta Vault.exe");
                if path.exists() {
                    return Command::new(&path)
                        .spawn()
                        .map(|_| ())
                        .map_err(|e| format!("Could not open Flowsta Vault: {e}"));
                }
            }
        }
        Err("Could not find Flowsta Vault. Please open it manually.".into())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err("Unsupported platform".into())
    }
}

/// Runtime environment string for the "Same here" corroboration stamp,
/// e.g. "macOS 26.1 (arm64)". Read from the HOST, not the webview: a
/// WKWebView UA freezes the macOS version ("10_15_7" forever), so a true
/// version must come from os_info here. Infallible - worst case os_info
/// yields "Unknown", which is still a usable (if vague) stamp.
#[tauri::command]
pub fn app_environment() -> String {
    let info = os_info::get();

    // os_info renders Type::Macos as "Mac OS"; prefer the conventional
    // "macOS" spelling for the stamp. Other types render fine as-is.
    let os = match info.os_type() {
        os_info::Type::Macos => "macOS".to_string(),
        other => other.to_string(),
    };

    let version = info.version().to_string();
    let base = if version.is_empty() || version == "Unknown" {
        os
    } else {
        format!("{os} {version}")
    };

    match info.architecture() {
        Some(arch) => format!("{base} ({arch})"),
        None => base,
    }
}

// ── Poll commands (replace with your app's commands) ──────────────────
//
// Each command follows the same pattern:
//   1. Lock the AppWebsocket from state
//   2. Parse input (convert string hashes to ActionHash)
//   3. Encode payload with ExternIO::encode()
//   4. Call call_zome() with your zome name and function
//   5. Decode the result and return a frontend-friendly type

#[tauri::command]
pub async fn create_item(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    kind: ItemKind,
    campaign: String,
    section: String,
    title: String,
    instructions: String,
    look_for: String,
    order: u32,
    admin_grant_action_hash: Option<String>,
) -> Result<String, String> {
    // Owner-seeded — require a linked Flowsta identity (frontend gating
    // alone is bypassable).
    if load_identity_link(&state.data_dir).is_none() {
        return Err("Sign in with Flowsta to create scenarios".to_string());
    }

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    // For Scenario items, require an admin grant
    if kind == ItemKind::Scenario && admin_grant_action_hash.is_none() {
        return Err("Scenario items require an admin_grant_action_hash".to_string());
    }

    // Manually encode the input (we can't import CreateItemInput from the zome)
    let input = serde_json::json!({
        "kind": kind,
        "admin_grant_action_hash": admin_grant_action_hash,
        "campaign": campaign,
        "section": section,
        "title": title,
        "instructions": instructions,
        "look_for": look_for,
        "order": order,
    });
    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "create_item", payload).await?;

    let action_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(action_hash.to_string())
}

/// Bulk-create scenarios from a parsed Markdown campaign. The frontend
/// parses the document into many CreateItemInput and sends them in one
/// call; the zome returns the number created.
/// Each item must have admin_grant_action_hash for Scenario kind.
#[tauri::command]
pub async fn import_items(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    items: Vec<serde_json::Value>,
) -> Result<u32, String> {
    if load_identity_link(&state.data_dir).is_none() {
        return Err("Sign in with Flowsta to import scenarios".to_string());
    }

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    // Validate that Scenario items have admin_grant_action_hash
    for item in &items {
        let kind = item.get("kind").and_then(|k| k.as_str());
        let has_grant = item.get("admin_grant_action_hash").is_some();
        if kind == Some("Scenario") && !has_grant {
            return Err("All Scenario items require admin_grant_action_hash".to_string());
        }
    }

    let payload = ExternIO::encode(items).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "import_items", payload).await?;

    let count: u32 = result.decode().map_err(|e| e.to_string())?;
    Ok(count)
}

#[tauri::command]
pub async fn get_item(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    action_hash: String,
) -> Result<Option<ItemDetail>, String> {
    let hash = ActionHash::try_from(action_hash)
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_item", payload).await?;
    let record: Option<Record> = result.decode().map_err(|e| e.to_string())?;

    match record {
        Some(record) => {
            let item: Item = decode_entry(&record)?;
            Ok(Some(ItemDetail {
                item,
                author: record.action().author().to_string(),
            }))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn get_all_items(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<ItemListItem>, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(()).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_all_items", payload).await?;
    let records: Vec<Record> = result.decode().map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for record in &records {
        if let Ok(item) = decode_entry::<Item>(record) {
            items.push(ItemListItem {
                hash: record.action_address().to_string(),
                item,
                author: record.action().author().to_string(),
            });
        }
    }
    Ok(items)
}

#[tauri::command]
pub async fn archive_item(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    action_hash: String,
) -> Result<String, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let hash = ActionHash::try_from(action_hash).map_err(|e| format!("Invalid action hash: {:?}", e))?;
    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "archive_item", payload).await?;

    let archive_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(archive_hash.to_string())
}

#[tauri::command]
pub async fn get_archived_items(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<ItemListItem>, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(()).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_archived_items", payload).await?;
    let records: Vec<Record> = result.decode().map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for record in &records {
        if let Ok(item) = decode_entry::<Item>(record) {
            items.push(ItemListItem {
                hash: record.action_address().to_string(),
                item,
                author: record.action().author().to_string(),
            });
        }
    }
    Ok(items)
}

#[tauri::command]
pub async fn unarchive_item(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    action_hash: String,
) -> Result<String, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let hash = ActionHash::try_from(action_hash).map_err(|e| format!("Invalid action hash: {:?}", e))?;
    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "unarchive_item", payload).await?;

    let unarchive_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(unarchive_hash.to_string())
}

#[allow(dead_code)] // dormant: no Item delete in v0.0.1 (unregistered)
#[tauri::command]
pub async fn delete_poll(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    action_hash: String,
) -> Result<String, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let hash =
        ActionHash::try_from(action_hash).map_err(|e| format!("Invalid action hash: {:?}", e))?;
    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "delete_poll", payload).await?;

    let delete_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(delete_hash.to_string())
}

#[tauri::command]
pub async fn respond(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    item_action_hash: String,
    verdict: Verdict,
) -> Result<String, String> {
    // Require a linked Flowsta identity — frontend gating alone is bypassable.
    if load_identity_link(&state.data_dir).is_none() {
        return Err("Sign in with Flowsta to respond".to_string());
    }

    let hash = ActionHash::try_from(item_action_hash)
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let input = RespondInput {
        item_action_hash: hash,
        verdict,
    };
    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "respond", payload).await?;

    let action_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(action_hash.to_string())
}

#[tauri::command]
pub async fn get_item_responses(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    item_action_hash: String,
) -> Result<Vec<ResponseData>, String> {
    let hash = ActionHash::try_from(item_action_hash)
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_item_responses", payload).await?;
    let records: Vec<Record> = result.decode().map_err(|e| e.to_string())?;

    let mut responses = Vec::new();
    for record in &records {
        let response: Response = decode_entry(record)?;
        responses.push(ResponseData {
            hash: record.action_address().to_string(),
            item_action_hash: response.item_action_hash.to_string(),
            verdict: response.verdict,
            author: record.action().author().to_string(),
            created_at: response.created_at,
        });
    }
    Ok(responses)
}

/// Add a free-text finding to an item. Many per agent; append-only. Plaintext
/// on the DHT for v0.0.1 (cohort encryption is a later, additive layer).
#[tauri::command]
pub async fn create_finding(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    item_action_hash: String,
    text: String,
) -> Result<String, String> {
    if load_identity_link(&state.data_dir).is_none() {
        return Err("Sign in with Flowsta to add a finding".to_string());
    }

    let hash = ActionHash::try_from(item_action_hash)
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let input = CreateFindingInput {
        item_action_hash: hash,
        text,
    };
    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "create_finding", payload).await?;

    let action_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(action_hash.to_string())
}

#[tauri::command]
pub async fn get_item_findings(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    item_action_hash: String,
) -> Result<Vec<FindingData>, String> {
    let hash = ActionHash::try_from(item_action_hash)
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_item_findings", payload).await?;
    let records: Vec<Record> = result.decode().map_err(|e| e.to_string())?;

    let mut findings = Vec::new();
    for record in &records {
        let finding: Finding = decode_entry(record)?;
        findings.push(FindingData {
            hash: record.action_address().to_string(),
            item_action_hash: finding.item_action_hash.to_string(),
            text: finding.text,
            author: record.action().author().to_string(),
            created_at: finding.created_at,
        });
    }
    Ok(findings)
}

// ── Identity link persistence (Flowsta infrastructure — keep as-is) ───
//
// The identity link is persisted to a JSON file so the app can detect
// a previously-linked Vault user across restarts and DNA migrations.
// This data is separate from the DHT — it's local to this device.
//
// Two files work together:
//   - identity-link.json — stores vault_agent_pub_key + entry_action_hash
//     (proves this user linked with Flowsta; survives DNA migrations)
//   - profile-cache.json — stores display_name + profile_picture
//     (allows the app to show the user's identity without Vault running)

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct IdentityLinkData {
    pub vault_agent_pub_key: String,
    pub entry_action_hash: String,
    pub linked_at: i64,
}

fn identity_link_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("identity-link.json")
}

fn load_identity_link(data_dir: &std::path::Path) -> Option<IdentityLinkData> {
    let path = identity_link_path(data_dir);
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

fn save_identity_link(data_dir: &std::path::Path, data: &IdentityLinkData) {
    let path = identity_link_path(data_dir);
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = std::fs::write(path, json);
    }
}

// ── Profile persistence (survives Vault being closed/locked) ──────────
//
// The Flowsta Vault only needs to be running for the initial identity
// linking ceremony. After that, the display name and profile picture
// are cached locally in profile-cache.json. On subsequent startups,
// the app loads from cache immediately — no Vault dependency.
//
// The cache is refreshed whenever the Vault is available (layout.tsx
// polls /status every 3s). If the user changes their name or picture
// in the Vault, the cache updates automatically on next poll.

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CachedProfile {
    pub display_name: Option<String>,
    pub profile_picture: Option<String>,
}

fn profile_cache_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("profile-cache.json")
}

fn load_cached_profile(data_dir: &std::path::Path) -> Option<CachedProfile> {
    let path = profile_cache_path(data_dir);
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

fn save_cached_profile(data_dir: &std::path::Path, profile: &CachedProfile) {
    let path = profile_cache_path(data_dir);
    if let Ok(json) = serde_json::to_string_pretty(profile) {
        let _ = std::fs::write(path, json);
    }
}

fn delete_identity_link(data_dir: &std::path::Path) {
    let path = identity_link_path(data_dir);
    let _ = std::fs::remove_file(path);
}

#[tauri::command]
pub fn get_cached_profile(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Option<CachedProfile> {
    load_cached_profile(&state.data_dir)
}

#[tauri::command]
pub fn save_profile_cache(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    display_name: Option<String>,
    profile_picture: Option<String>,
) {
    save_cached_profile(
        &state.data_dir,
        &CachedProfile {
            display_name,
            profile_picture,
        },
    );
}

// ── Identity linking commands (Flowsta infrastructure — keep as-is) ───
//
// These commands handle the Flowsta Vault ↔ app identity linking ceremony.
// The Vault signs an attestation that this app's agent key belongs to the
// same person as the Vault's agent key. This attestation is stored on the
// DHT via the agent_linking zome so anyone can verify it.

#[tauri::command]
pub async fn commit_identity_link(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    vault_agent_pub_key: String,
    vault_signature: String,
) -> Result<String, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let external_agent = parse_agent_pub_key_string(&vault_agent_pub_key)?;

    // Decode base64 signature to bytes.
    use base64::Engine;
    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(&vault_signature)
        .map_err(|e| format!("Invalid signature: {}", e))?;

    #[derive(serde::Serialize, Debug)]
    struct CreateExternalLinkInput {
        external_agent: AgentPubKey,
        external_signature: Vec<u8>,
    }

    let input = CreateExternalLinkInput {
        external_agent,
        external_signature: sig_bytes,
    };
    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result = call_zome(client, AGENT_LINKING_ZOME, "create_external_link", payload).await?;

    let action_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    let action_hash_str = action_hash.to_string();

    // Persist the link data for later revocation
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    save_identity_link(
        &state.data_dir,
        &IdentityLinkData {
            vault_agent_pub_key: vault_agent_pub_key.clone(),
            entry_action_hash: action_hash_str.clone(),
            linked_at: now,
        },
    );

    Ok(action_hash_str)
}

#[tauri::command]
pub fn get_identity_link(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Option<IdentityLinkData> {
    load_identity_link(&state.data_dir)
}

#[tauri::command]
pub async fn revoke_identity_link(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<(), String> {
    let Some(link_data) = load_identity_link(&state.data_dir) else {
        // Already unlinked locally — nothing to do.
        return Ok(());
    };

    // Call revoke_link on the agent_linking zome
    let action_hash = ActionHash::try_from(link_data.entry_action_hash.clone())
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(action_hash).map_err(|e| e.to_string())?;
    call_zome(client, AGENT_LINKING_ZOME, "revoke_link", payload).await?;

    // Delete local persistence
    delete_identity_link(&state.data_dir);

    // Best-effort: notify Vault via IPC
    let agent_key = state.agent_pub_key.lock().unwrap().clone();
    if let Some(agent_key) = agent_key {
        let _ = notify_vault_revoke("ProofPoll", &agent_key).await;
    }

    Ok(())
}

/// Best-effort notification to Vault that identity link was revoked.
async fn notify_vault_revoke(app_name: &str, app_agent_pub_key: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "app_name": app_name,
        "app_agent_pub_key": app_agent_pub_key,
    });

    let _ = client
        .post("http://127.0.0.1:27777/revoke-identity")
        .json(&body)
        .send()
        .await;

    Ok(())
}

/// Core lookup: the agents the agent-linking zome reports as directly linked
/// to `agent` (an already-parsed key). Returns `AgentPubKey`s, NOT strings, so
/// callers can chain hops without round-tripping through the string form —
/// which matters because the Flowsta Vault's agent key fails holo_hash's strict
/// string parser (BadChecksum), even though its raw bytes are a valid key. The
/// zome hands keys back as valid bytes, so chaining from those always works.
/// Locks the app client itself; callers must NOT already hold that lock.
async fn linked_agent_keys(
    state: &std::sync::Arc<AppState>,
    agent: &AgentPubKey,
) -> Result<Vec<AgentPubKey>, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(agent.clone()).map_err(|e| e.to_string())?;
    let result = call_zome(client, AGENT_LINKING_ZOME, "get_linked_agents", payload).await?;
    result.decode().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_linked_agents(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    agent_pub_key: String,
) -> Result<Vec<String>, String> {
    let agent = AgentPubKey::try_from(agent_pub_key.clone())
        .map_err(|e| format!("Invalid agent key: {:?}", e))?;
    let out: Vec<String> = linked_agent_keys(state.inner(), &agent)
        .await?
        .iter()
        .map(|a| a.to_string())
        .collect();
    // Debug, not info: the layout polls this every ~15s, so info would spam
    // the log. The user-facing identity set is logged (on change) by
    // get_my_agent_set instead.
    log::debug!(
        "[identity] get_linked_agents({}) -> {} agent(s): {:?}",
        agent_pub_key, out.len(), out,
    );
    Ok(out)
}

/// Last set logged by `get_my_agent_set`, so we log only when it changes
/// (the command runs ~every 30s; logging every call would spam the log).
static LAST_AGENT_SET: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

/// Every Holochain agent key that belongs to THIS user: the local conductor
/// agent plus every other ProofPoll agent linked to the same Flowsta Vault
/// identity (i.e. the user's other installs/devices). Used for RECOGNITION
/// only — "is this poll/vote/flag mine?" — never for mutation (Holochain only
/// lets the original author edit/delete, so those gates stay bound to the
/// current local agent).
///
/// This lives in Rust rather than being orchestrated from the webview so the
/// lookup is (a) a single robust round-trip instead of a fragile multi-call
/// frontend sequence that a poll-fetch failure could skip, and (b) fully
/// observable in proofpoll.log (webview console logs aren't readable on disk).
///
/// Resolved as a 2-hop walk through the link graph: local agent → the Vault
/// identity hub(s) it's linked to → every agent linked to that hub. We get the
/// hub key from the zome (valid bytes) rather than parsing the Vault key string
/// from identity-link.json, because that string fails holo_hash's checksum
/// parser. Sibling agents arrive via DHT gossip, so the set grows over the
/// first few minutes after launch — callers should re-run it.
#[tauri::command]
pub async fn get_my_agent_set(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    local_agent: Option<String>,
) -> Result<Vec<String>, String> {
    let local = match local_agent.as_ref().and_then(|s| AgentPubKey::try_from(s.clone()).ok()) {
        Some(k) => k,
        None => {
            log::warn!("[identity] get_my_agent_set: no valid local agent: {:?}", local_agent);
            return Ok(local_agent.into_iter().collect());
        }
    };

    let mut set: Vec<AgentPubKey> = vec![local.clone()];

    // Hop 1: which identity hub(s) is this local agent linked to (the Vault).
    // Hop 2: every agent linked to that hub = all of the user's installs.
    match linked_agent_keys(state.inner(), &local).await {
        Ok(hubs) => {
            for hub in hubs {
                match linked_agent_keys(state.inner(), &hub).await {
                    Ok(siblings) => {
                        for s in siblings {
                            if !set.contains(&s) {
                                set.push(s);
                            }
                        }
                    }
                    Err(e) => log::warn!("[identity] get_my_agent_set: hub lookup failed: {}", e),
                }
            }
        }
        // Conductor not ready — degrade to the local agent. The caller
        // re-runs on its interval, so this self-heals as the network comes up.
        Err(e) => log::warn!("[identity] get_my_agent_set: local lookup failed: {}", e),
    }

    let mut out: Vec<String> = set.iter().map(|a| a.to_string()).collect();
    out.sort(); // stable order so change-detection doesn't fire on reordering

    // Log only when the set CHANGES (first resolve, or a new linked agent
    // gossips in). Keeps the observability that diagnosed the BadChecksum bug
    // without logging every ~30s tick.
    {
        let mut last = LAST_AGENT_SET.lock().unwrap();
        if last.as_deref() != Some(out.as_slice()) {
            log::info!(
                "[identity] get_my_agent_set: local={:?} -> {} agent(s): {:?}",
                local_agent, out.len(), out,
            );
            *last = Some(out.clone());
        }
    }
    Ok(out)
}

// ── Flag types and commands (v1.1 — replace with your moderation system) ──
//
// Community flagging: users can flag content with a reason. The UI hides
// content that exceeds a configurable flag threshold. Data is never deleted
// from the DHT — censorship resistance is preserved at the data layer,
// moderation happens at the UI layer.

#[derive(serde::Serialize, Clone)]
pub struct FlagData {
    pub hash: String,
    pub flag: FlagResponse,
    pub author: String,
}

#[derive(serde::Serialize, Clone)]
pub struct FlagResponse {
    pub poll_action_hash: String,
    pub reason: String,
    pub created_at: i64,
}

#[derive(serde::Deserialize)]
struct FlagEntry {
    poll_action_hash: ActionHash,
    reason: FlagReason,
    created_at: i64,
}

#[derive(serde::Deserialize, Debug)]
enum FlagReason {
    Spam,
    Misleading,
    OffTopic,
    Inappropriate,
}

impl std::fmt::Display for FlagReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlagReason::Spam => write!(f, "Spam"),
            FlagReason::Misleading => write!(f, "Misleading"),
            FlagReason::OffTopic => write!(f, "OffTopic"),
            FlagReason::Inappropriate => write!(f, "Inappropriate"),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct FlagPollInput {
    poll_action_hash: ActionHash,
    reason: String,
}

#[tauri::command]
pub async fn flag_poll(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    poll_action_hash: String,
    reason: String,
) -> Result<String, String> {
    // Require a linked Flowsta identity — frontend gating alone is bypassable.
    if load_identity_link(&state.data_dir).is_none() {
        return Err("Sign in with Flowsta to flag polls".to_string());
    }

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let hash = ActionHash::try_from(poll_action_hash)
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;

    #[derive(serde::Serialize, Debug)]
    struct ZomeFlagInput {
        poll_action_hash: ActionHash,
        reason: serde_json::Value,
    }

    let zome_input = ZomeFlagInput {
        poll_action_hash: hash,
        reason: serde_json::Value::String(reason),
    };

    let payload = ExternIO::encode(zome_input).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "flag_poll", payload).await?;

    let action_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(action_hash.to_string())
}

#[tauri::command]
pub async fn get_poll_flags(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    poll_action_hash: String,
) -> Result<Vec<FlagData>, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let hash = ActionHash::try_from(poll_action_hash)
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;
    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_poll_flags", payload).await?;

    let records: Vec<Record> = result.decode().map_err(|e| e.to_string())?;
    let mut flags = Vec::new();
    for record in &records {
        let flag: FlagEntry = decode_entry(record)?;
        flags.push(FlagData {
            hash: record.action_address().to_string(),
            flag: FlagResponse {
                poll_action_hash: flag.poll_action_hash.to_string(),
                reason: format!("{}", flag.reason),
                created_at: flag.created_at,
            },
            author: record.action().author().to_string(),
        });
    }
    Ok(flags)
}

#[tauri::command]
pub async fn remove_flag(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    flag_action_hash: String,
) -> Result<String, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let hash = ActionHash::try_from(flag_action_hash)
        .map_err(|e| format!("Invalid action hash: {:?}", e))?;
    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "remove_flag", payload).await?;

    let delete_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(delete_hash.to_string())
}

#[tauri::command]
pub async fn get_flag_threshold(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<u32, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(()).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_flag_threshold", payload).await?;

    let threshold: u32 = result.decode().map_err(|e| e.to_string())?;
    Ok(threshold)
}

// ── Migration status commands (infrastructure — keep as-is) ───────────
//
// These let the frontend show migration progress to the user.
// The actual migration logic is in migration.rs.

#[tauri::command]
pub async fn get_migration_status(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<crate::migration::MigrationState, String> {
    let state = state.migration_state.lock().await;
    Ok(state.clone())
}

#[tauri::command]
pub async fn abandon_pending_votes(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<(), String> {
    let mut migration = state.migration_state.lock().await;
    migration.votes_pending.clear();
    migration.save(&state.data_dir);
    Ok(())
}

// ── Encrypted entry commands (v1.3) ──────────────────────────────────

/// Extract the raw 32-byte Ed25519 public key from the agent key string.
/// AgentPubKey is 39 bytes: 3-byte prefix + 32-byte key + 4-byte DHT location.
fn get_agent_ed25519_bytes(state: &AppState) -> Result<[u8; 32], String> {
    let key_str = state
        .agent_pub_key
        .lock()
        .unwrap()
        .clone()
        .ok_or("Agent key not available")?;
    let agent_key = parse_agent_pub_key_string(&key_str)?;
    let raw = agent_key.get_raw_39();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&raw[3..35]);
    Ok(bytes)
}

/// Encrypted entry as decoded from a DHT Record.
#[derive(serde::Deserialize)]
struct EncryptedEntryData {
    cipher: Vec<u8>,
    nonce: Vec<u8>,
    entry_type_hint: String,
    related_hash: Option<ActionHash>,
}

/// Draft poll as returned to the frontend (decrypted).
#[derive(serde::Serialize)]
pub struct DraftPollItem {
    pub hash: String,
    pub title: String,
    pub description: String,
    pub options: Vec<String>,
    pub closes_at: Option<i64>,
    pub poll_type: String,
    pub created_at: i64,
}

/// Save a private vote rationale (encrypted on the DHT).
#[tauri::command]
pub async fn save_vote_rationale(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    vote_action_hash: String,
    rationale_text: String,
) -> Result<String, String> {
    let agent_bytes = get_agent_ed25519_bytes(&state)?;

    // Encrypt the rationale — acquire lair first, then app client (lock ordering)
    let (nonce, cipher) = {
        let lair = state.lair_client.lock().await;
        let lair = lair.as_ref().ok_or("Lair not connected")?;
        crate::crypto::encrypt_to_self(lair, agent_bytes, rationale_text.as_bytes()).await?
    };

    let vote_hash = parse_action_hash(&vote_action_hash)?;

    #[derive(serde::Serialize, Debug)]
    struct Input {
        cipher: Vec<u8>,
        nonce: Vec<u8>,
        link_as: String,
        related_hash: Option<ActionHash>,
    }

    let input = Input {
        cipher,
        nonce: nonce.to_vec(),
        link_as: "vote_rationale".to_string(),
        related_hash: Some(vote_hash),
    };

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;
    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "create_encrypted_entry", payload).await?;
    let hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(hash.to_string())
}

/// Get and decrypt a vote rationale.
#[tauri::command]
pub async fn get_vote_rationale(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    vote_action_hash: String,
) -> Result<Option<String>, String> {
    let vote_hash = parse_action_hash(&vote_action_hash)?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;
    let payload = ExternIO::encode(vote_hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_vote_rationale", payload).await?;
    let record: Option<Record> = result.decode().map_err(|e| e.to_string())?;

    let record = match record {
        Some(r) => r,
        None => return Ok(None),
    };

    // Only decrypt if we're the author
    let my_agent = state.agent_pub_key.lock().unwrap().clone();
    if my_agent.as_deref() != Some(&record.action().author().to_string()) {
        return Ok(None);
    }

    let ee: EncryptedEntryData = decode_entry(&record)?;
    let agent_bytes = get_agent_ed25519_bytes(&state)?;

    let mut nonce = [0u8; 24];
    if ee.nonce.len() != 24 {
        return Err("Invalid nonce length".into());
    }
    nonce.copy_from_slice(&ee.nonce);

    let lair = state.lair_client.lock().await;
    let lair = lair.as_ref().ok_or("Lair not connected")?;
    let plaintext = crate::crypto::decrypt_from_self(lair, agent_bytes, nonce, &ee.cipher).await?;
    let text = String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8: {}", e))?;
    Ok(Some(text))
}

/// Save a draft poll (encrypted on the DHT).
#[tauri::command]
pub async fn save_draft_poll(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    title: String,
    description: String,
    options: Vec<String>,
    closes_at: Option<i64>,
    poll_type: Option<String>,
) -> Result<String, String> {
    let agent_bytes = get_agent_ed25519_bytes(&state)?;

    let draft = serde_json::json!({
        "title": title,
        "description": description,
        "options": options,
        "closes_at": closes_at,
        "poll_type": poll_type.unwrap_or_else(|| "Anonymous".to_string()),
    });
    let plaintext = serde_json::to_vec(&draft).map_err(|e| e.to_string())?;

    let (nonce, cipher) = {
        let lair = state.lair_client.lock().await;
        let lair = lair.as_ref().ok_or("Lair not connected")?;
        crate::crypto::encrypt_to_self(lair, agent_bytes, &plaintext).await?
    };
    #[derive(serde::Serialize, Debug)]
    struct Input {
        cipher: Vec<u8>,
        nonce: Vec<u8>,
        link_as: String,
        related_hash: Option<ActionHash>,
    }

    let input = Input {
        cipher,
        nonce: nonce.to_vec(),
        link_as: "draft_poll".to_string(),
        related_hash: None,
    };

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;
    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "create_encrypted_entry", payload).await?;
    let hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(hash.to_string())
}

/// Get all draft polls (decrypted).
#[tauri::command]
pub async fn get_my_drafts(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<DraftPollItem>, String> {
    log::info!("get_my_drafts: fetching from DHT...");
    let records: Vec<Record> = {
        let client = state.app_client.lock().await;
        let client = client.as_ref().ok_or("Conductor not ready")?;
        let payload = ExternIO::encode(()).map_err(|e| e.to_string())?;
        let result = call_zome(client, POLLS_ZOME, "get_my_drafts", payload).await?;
        result.decode().map_err(|e| e.to_string())?
    }; // app_client lock released here

    log::info!("get_my_drafts: {} records found, decrypting...", records.len());
    let agent_bytes = get_agent_ed25519_bytes(&state)?;
    let lair = state.lair_client.lock().await;
    let lair = lair.as_ref().ok_or("Lair not connected")?;

    let mut drafts = Vec::new();
    for record in &records {
        let hash = record.action_address().to_string();
        let created_at = record.action().timestamp().as_seconds_and_nanos().0;

        let ee: EncryptedEntryData = match decode_entry(record) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut nonce = [0u8; 24];
        if ee.nonce.len() != 24 {
            continue;
        }
        nonce.copy_from_slice(&ee.nonce);

        let plaintext = match crate::crypto::decrypt_from_self(lair, agent_bytes, nonce, &ee.cipher).await {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Failed to decrypt draft {}: {}", hash, e);
                continue;
            }
        };

        let draft: serde_json::Value = match serde_json::from_slice(&plaintext) {
            Ok(v) => v,
            Err(_) => continue,
        };

        drafts.push(DraftPollItem {
            hash,
            title: draft["title"].as_str().unwrap_or("").to_string(),
            description: draft["description"].as_str().unwrap_or("").to_string(),
            options: draft["options"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            closes_at: draft["closes_at"].as_i64(),
            poll_type: draft["poll_type"].as_str().unwrap_or("Anonymous").to_string(),
            created_at,
        });
    }

    Ok(drafts)
}

/// Publish a draft: decrypt it, create a real poll, delete the draft.
#[tauri::command]
pub async fn publish_draft(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    draft_action_hash: String,
) -> Result<String, String> {
    let draft_hash = parse_action_hash(&draft_action_hash)?;
    let agent_bytes = get_agent_ed25519_bytes(&state)?;

    // Fetch and decrypt the draft
    let (draft_json, _ee) = {
        let client = state.app_client.lock().await;
        let client = client.as_ref().ok_or("Conductor not ready")?;
        let payload = ExternIO::encode(draft_hash.clone()).map_err(|e| e.to_string())?;
        let result = call_zome(client, POLLS_ZOME, "get_poll", payload).await?;
        let record: Option<Record> = result.decode().map_err(|e| e.to_string())?;
        let record = record.ok_or("Draft not found")?;

        let ee: EncryptedEntryData = decode_entry(&record)?;
        let mut nonce = [0u8; 24];
        if ee.nonce.len() != 24 {
            return Err("Invalid nonce".into());
        }
        nonce.copy_from_slice(&ee.nonce);

        let lair = state.lair_client.lock().await;
        let lair = lair.as_ref().ok_or("Lair not connected")?;
        let plaintext = crate::crypto::decrypt_from_self(lair, agent_bytes, nonce, &ee.cipher).await?;
        let json: serde_json::Value = serde_json::from_slice(&plaintext)
            .map_err(|e| format!("Failed to parse draft: {}", e))?;
        (json, ee)
    };

    // Create the real poll using the existing CreatePollInput struct
    let input = CreatePollInput {
        title: draft_json["title"].as_str().unwrap_or("").to_string(),
        description: draft_json["description"].as_str().unwrap_or("").to_string(),
        options: draft_json["options"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        closes_at: draft_json["closes_at"].as_i64(),
        poll_type: Some(draft_json["poll_type"].as_str().unwrap_or("Anonymous").to_string()),
    };

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;
    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "create_poll", payload).await?;
    let poll_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;

    // Delete the draft
    let delete_payload = ExternIO::encode(draft_hash).map_err(|e| e.to_string())?;
    if let Err(e) = call_zome(client, POLLS_ZOME, "delete_encrypted_entry", delete_payload).await {
        log::warn!("Failed to delete draft after publishing: {}", e);
    }

    Ok(poll_hash.to_string())
}

/// Delete a draft poll.
#[tauri::command]
pub async fn delete_draft(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    draft_action_hash: String,
) -> Result<String, String> {
    let hash = parse_action_hash(&draft_action_hash)?;
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;
    let payload = ExternIO::encode(hash).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "delete_encrypted_entry", payload).await?;
    let deleted: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(deleted.to_string())
}

/// Parse an action hash string into an ActionHash.
fn parse_action_hash(s: &str) -> Result<ActionHash, String> {
    ActionHash::try_from(s.to_string()).map_err(|e| format!("Invalid action hash: {:?}", e))
}

// ── CAL-compliant backup helpers (v0.2.0+) ────────────────────────
//
// `decode_record_for_export` decodes an entry by type into plain JSON so the
// Flowsta Vault can embed a human-readable view of each record in the user's
// data export — what the Cryptographic Autonomy License (§4.2.1) obliges
// every Holochain app to provide.
//
// Adding a new entry type to ProofPoll means adding ONE `match` arm in
// `decode_record_for_export` and one in `build_canonical_backup`'s record loop,
// mirroring the entry type's existing structs. There is no restore command —
// see the note below on why recovery is recognition, not replay.

use base64::Engine as _;

/// Decode an entry's MessagePack bytes into a human-readable JSON view, used
/// when the SDK builds a backup for Vault. Each arm uses the existing local
/// struct (Poll / Vote) so all fields stay in sync with the DNA via serde.
#[tauri::command]
pub async fn decode_record_for_export(
    entry_type: String,
    entry_bytes_b64: String,
) -> Result<serde_json::Value, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&entry_bytes_b64)
        .map_err(|e| format!("base64 decode: {}", e))?;

    match entry_type.as_str() {
        "Item" => {
            let item: Item = rmp_serde::from_slice(&bytes)
                .map_err(|e| format!("Item decode: {}", e))?;
            serde_json::to_value(item).map_err(|e| e.to_string())
        }
        "Response" => {
            let response: Response = rmp_serde::from_slice(&bytes)
                .map_err(|e| format!("Response decode: {}", e))?;
            serde_json::to_value(response).map_err(|e| e.to_string())
        }
        "Finding" => {
            let finding: Finding = rmp_serde::from_slice(&bytes)
                .map_err(|e| format!("Finding decode: {}", e))?;
            serde_json::to_value(finding).map_err(|e| e.to_string())
        }
        other => Ok(serde_json::json!({
            "_warning": format!("Unknown entry type: {}", other),
            "raw_bytes_hex": hex::encode(&bytes),
        })),
    }
}

// Note: there is no restore command. The old `restore_record` dispatcher — and a
// later source-chain graft attempt — were both removed. Recovery is now
// identity-aware recognition: a fresh install reads the user's existing DHT data
// via their linked agent set (`get_my_agent_set`, a 2-hop walk of the identity
// link graph), so nothing is re-created or duplicated. The backup feeds the CAL
// §4.2.1 data export only.

/// Build the user's canonical-shape backup payload for Vault.
///
/// Replaces `get_export_data` for the Vault auto-backup path. Produces the
/// same payload shape the SDK's `dumpCellStateForBackup` would produce, so
/// the Flowsta Vault recognises it as canonical and:
///   - persists a per-entry-type summary alongside the backup metadata
///     (rendered as "12 polls, 38 votes" on the Your Data and overview UIs),
///   - inlines the `human_readable` view of each record into the user's CAL
///     §4.2.1 data export (full or per-app), and
///   - leaves the signed `raw_record` inside the encrypted backup.
///
/// Architecture note: this captures the user's ENTIRE source chain via the
/// admin `dump_full_state`, not just Poll/Vote app entries — so the CAL export
/// is the user's complete, signed chain. Each `raw_record` is a verbatim
/// `SourceChainDumpRecord` (a fully portable signed record).
///
/// On top of the raw records we layer the human-readable view + per-type
/// counts for the app entries we can decode (Poll, Vote) — this is what Vault
/// renders on the Your Data page and inlines into the CAL §4.2.1 export.
/// Infrastructure records (Dna, AgentValidationPkg, CapGrant, …) carry their
/// raw_record but no human_readable (they're chain plumbing, not user data).
///
/// The three lair files are appended at the top level so the backup is also
/// CAL-complete: data PLUS the cryptographic keys needed to operate it
/// independently. See `read_lair_backup_fields`.
#[tauri::command]
pub async fn build_canonical_backup(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    use holochain_client::{AdminWebsocket, CellInfo};

    let my_key = {
        let key = state.agent_pub_key.lock().unwrap();
        key.clone().ok_or("Agent key not available")?
    };

    // 1. Connect to the admin websocket (same pattern as try_reenable_app).
    let admin_ws = AdminWebsocket::connect(
        format!("localhost:{}", crate::conductor::ADMIN_WS_PORT),
        Some("proofpoll".to_string()),
    )
    .await
    .map_err(|e| format!("Failed to connect to admin WS for backup: {}", e))?;

    // 2. Find the active (v1.3) provisioned cell.
    let apps = admin_ws
        .list_apps(None)
        .await
        .map_err(|e| format!("list_apps: {}", e))?;
    let cell_id = apps
        .iter()
        .find(|a| a.installed_app_id == crate::dna::ACTIVE_APP_ID)
        .and_then(|app| {
            app.cell_info
                .values()
                .flat_map(|cells| cells.iter())
                .find_map(|c| match c {
                    CellInfo::Provisioned(p) => Some(p.cell_id.clone()),
                    _ => None,
                })
        })
        .ok_or("Active app cell not found for backup")?;

    // 3. Dump the full source chain (every action authored by this agent).
    let dump = admin_ws
        .dump_full_state(cell_id, None)
        .await
        .map_err(|e| format!("dump_full_state: {}", e))?;

    // 4. Build canonical records + per-type counts.
    let mut records: Vec<serde_json::Value> = Vec::new();
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();

    for rec in &dump.source_chain_dump.records {
        let action_hash = rec.action_address.to_string();
        let created_ms = rec.action.timestamp().as_millis();

        let (entry_type_name, human_readable) = classify_dump_record(rec);

        // Count only the decodable user-data entry types (those with a
        // non-null human_readable view). Infrastructure records keep their
        // signed `raw_record` so the export is a complete copy of the chain,
        // but they don't inflate the user-facing summary.
        if !human_readable.is_null() {
            *counts.entry(entry_type_name.clone()).or_insert(0) += 1;
        }

        let raw_record = serde_json::to_value(rec)
            .map_err(|e| format!("serialize SourceChainDumpRecord: {}", e))?;

        records.push(serde_json::json!({
            "entryType": entry_type_name,
            "actionHash": action_hash,
            "createdAtMs": created_ms,
            "human_readable": human_readable,
            "raw_record": raw_record,
        }));
    }

    let total_records: usize = counts.values().sum();
    let counts_json: serde_json::Map<String, serde_json::Value> = counts
        .into_iter()
        .map(|(k, v)| (k, serde_json::json!(v)))
        .collect();

    // 5. Assemble the canonical payload.
    let mut payload = serde_json::json!({
        "version": 1,
        "_readme": "Your Fieldnotes data, backed up automatically by Flowsta Vault. Encrypted with your device key — only you can read it. Each record below carries a plain-English view of what you authored AND a signed Holochain record for restore. The lair_* fields are the cryptographic keys that let you recover this identity on a fresh install.",
        "license": "Cryptographic Autonomy License v1.0 (CAL-1.0)",
        "app": {
            "name": "ProofPoll",
        },
        "agent_pub_key": my_key,
        "exported_at_iso": format!("unix:{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        ),
        "_summary": {
            "countsByEntryType": counts_json,
            "totalRecords": total_records,
        },
        "cells": [
            {
                "role_name": "polls",
                "_readme": "Each record below is one action on your source chain. `human_readable` is the plain-English view (app entries only). `raw_record` is the signed record used to restore your chain onto a fresh install.",
                "records": records,
            }
        ],
    });

    // 6. Append the lair recovery fields (data + keys = CAL-complete). If any
    //    of the three files is missing we omit all three — a partial set is
    //    useless for recovery and we'd rather ship a data-only backup than a
    //    misleading one.
    if let Some((passphrase, config_yaml, store_b64)) =
        read_lair_backup_fields(&state.data_dir)
    {
        payload["lair_passphrase"] = serde_json::json!(passphrase);
        payload["lair_keystore_config"] = serde_json::json!(config_yaml);
        payload["lair_keystore_data"] = serde_json::json!(store_b64);
    } else {
        log::warn!("build_canonical_backup: lair files incomplete; shipping data-only backup (no recovery fields)");
    }

    Ok(payload)
}

/// Classify a dumped source-chain record into a (entryType, human_readable)
/// pair for the canonical backup. App entries we can decode get a type name +
/// plain-JSON view; everything else gets an action-type label and a null
/// human_readable (its signed `raw_record` stays in the backup for a complete,
/// verifiable export, but it is not shown as user data).
///
/// For forking developers: the `(zome_index, entry_index)` table below mirrors
/// your DNA manifest's integrity-zome order and each integrity zome's
/// `EntryTypes` enum order. Update it alongside your entry types — it is the
/// one place that maps on-chain entry indices back to readable names.
///
/// ProofPoll v1.3 (`dna/v1.3/workdir/dna.yaml`):
///   integrity zomes: [0] agent_linking_integrity, [1] polls_integrity
///   polls_integrity EntryTypes: [0] Item [1] Response [2] Finding
fn classify_dump_record(
    rec: &holochain_state_types::SourceChainDumpRecord,
) -> (String, serde_json::Value) {
    use holochain_integrity_types::EntryType;

    let entry_type = match rec.action.entry_type() {
        Some(et) => et,
        // No entry reference — a system action (Dna, AgentValidationPkg,
        // InitZomesComplete, OpenChain, CloseChain, …). Label by the action
        // variant for diagnostics; not user data.
        None => return (action_variant_label(&rec.action), serde_json::Value::Null),
    };

    let app_def = match entry_type {
        EntryType::App(def) => def,
        EntryType::AgentPubKey => return ("AgentPubKey".to_string(), serde_json::Value::Null),
        EntryType::CapClaim => return ("CapClaim".to_string(), serde_json::Value::Null),
        EntryType::CapGrant => return ("CapGrant".to_string(), serde_json::Value::Null),
    };

    let zome = app_def.zome_index.0;
    let entry_idx = app_def.entry_index.0;

    // Pull the raw msgpack entry bytes (if the entry is present in the dump).
    let entry_bytes: Option<&[u8]> = rec
        .entry
        .as_ref()
        .and_then(|e| e.as_app_entry())
        .map(|app_bytes| app_bytes.as_ref().bytes().as_slice());

    match (zome, entry_idx) {
        // polls_integrity (zome index 1). NOTE the entry-index order follows
        // the EntryTypes enum: AdminGrant(0), Item(1), Response(2), Finding(3).
        // AdminGrant is an authority record, not user content — no human view.
        (1, 0) => ("AdminGrant".to_string(), serde_json::Value::Null),
        (1, 1) => decode_named::<Item>("Item", entry_bytes),
        (1, 2) => decode_named::<Response>("Response", entry_bytes),
        (1, 3) => decode_named::<Finding>("Finding", entry_bytes),
        // agent_linking_integrity (zome index 0)
        (0, 0) => ("IsSamePerson".to_string(), serde_json::Value::Null),
        _ => (
            format!("AppEntry(zome={},entry={})", zome, entry_idx),
            serde_json::Value::Null,
        ),
    }
}

/// Decode entry bytes into a named type for the human_readable view. Returns
/// (name, json) on success, or (name, null) if bytes are absent or undecodable
/// — the record is still kept (with its signed raw_record) so the export stays
/// a complete copy of the chain.
fn decode_named<T: serde::de::DeserializeOwned + serde::Serialize>(
    name: &str,
    entry_bytes: Option<&[u8]>,
) -> (String, serde_json::Value) {
    let human = entry_bytes
        .and_then(|b| rmp_serde::from_slice::<T>(b).ok())
        .and_then(|v| serde_json::to_value(v).ok())
        .unwrap_or(serde_json::Value::Null);
    (name.to_string(), human)
}

/// Best-effort label for a system (entry-less) action, used only for
/// diagnostics in the backup — never shown as user data.
fn action_variant_label(action: &holochain_integrity_types::Action) -> String {
    use holochain_integrity_types::Action;
    match action {
        Action::Dna(_) => "Dna",
        Action::AgentValidationPkg(_) => "AgentValidationPkg",
        Action::InitZomesComplete(_) => "InitZomesComplete",
        Action::CreateLink(_) => "CreateLink",
        Action::DeleteLink(_) => "DeleteLink",
        Action::OpenChain(_) => "OpenChain",
        Action::CloseChain(_) => "CloseChain",
        Action::Create(_) => "Create",
        Action::Update(_) => "Update",
        Action::Delete(_) => "Delete",
    }
    .to_string()
}

/// Read the three lair files and return them base64-encoded / stringified for
/// inclusion in the CAL §4.2.1 backup (the cryptographic keys the user needs to
/// operate their data independently). Returns `None` if any is missing — we
/// include all three or none.
// ── Administrator functions ────────────────────────────────────────

/// Commit an AdminGrant entry. The progenitor signature is produced by the
/// FRONTEND via Flowsta Vault's /sign endpoint (signed with the user's durable
/// device identity key — the progenitor), then passed down here. This host
/// command no longer signs anything itself; it only commits the grant. The
/// signature is verified in the integrity zome (validate_admin_grant) against
/// the progenitor pubkey burned into the DNA properties.
#[tauri::command]
pub async fn add_administrator(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    admin_pubkey_str: String,
    progenitor_signature: Vec<u8>,
) -> Result<String, String> {
    if admin_pubkey_str.trim().is_empty() {
        return Err("admin_pubkey_str must not be empty".to_string());
    }

    // Validate the pubkey parses (defensive; the zome re-checks).
    parse_agent_pub_key_string(&admin_pubkey_str)
        .map_err(|_| "Invalid admin pubkey format".to_string())?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    // Parse the admin pubkey into the typed AgentPubKey the zome expects.
    let admin_pubkey = parse_agent_pub_key_string(&admin_pubkey_str)
        .map_err(|_| "Invalid admin pubkey format".to_string())?;

    // Convert the 64-byte signature into a Signature. Encoding a TYPED struct
    // (not a serde_json::json! blob) is essential: AgentPubKey and Signature
    // must serialize in their native msgpack forms, or the zome's deserialize
    // produces a malformed Signature and verify_signature fails.
    let sig_array: [u8; 64] = progenitor_signature
        .as_slice()
        .try_into()
        .map_err(|_| format!("signature must be 64 bytes, got {}", progenitor_signature.len()))?;

    #[derive(serde::Serialize, Debug)]
    struct AddAdministratorInput {
        admin_pubkey: AgentPubKey,
        progenitor_signature: Signature,
    }

    let input = AddAdministratorInput {
        admin_pubkey,
        progenitor_signature: Signature(sig_array),
    };

    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "add_administrator", payload).await?;

    let action_hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(action_hash.to_string())
}

/// Return the 39 raw bytes of an AgentPubKey string, base64-encoded.
/// Used by the frontend to obtain the exact bytes that must be signed by
/// Flowsta Vault for an AdminGrant — the frontend keeps no @holochain/client,
/// so this byte-shaping stays in Rust. The integrity zome verifies the
/// progenitor signature over these same 39 bytes (admin_pubkey.get_raw_39()).
#[tauri::command]
pub fn pubkey_raw_b64(pubkey_str: String) -> Result<String, String> {
    use base64::Engine;
    let pubkey = parse_agent_pub_key_string(&pubkey_str)
        .map_err(|_| "Invalid pubkey format".to_string())?;
    let raw39 = pubkey.get_raw_39();
    Ok(base64::engine::general_purpose::STANDARD.encode(raw39))
}

/// Check if the current agent is an administrator.
#[tauri::command]
pub async fn is_administrator(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<bool, String> {
    // Resolve the local agent's pubkey to check if THEY are an admin
    let key_str = state
        .agent_pub_key
        .lock()
        .unwrap()
        .clone()
        .ok_or("Agent key not available")?;
    let local_agent = parse_agent_pub_key_string(&key_str)?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(local_agent).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "is_administrator", payload).await?;
    let is_admin: bool = result.decode().map_err(|e| e.to_string())?;
    Ok(is_admin)
}

/// Return the local agent's AdminGrant action hash (as a string), if they hold one.
/// The frontend attaches this to Scenario creation so validate can verify it.
#[tauri::command]
pub async fn get_admin_grant_hash(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Option<String>, String> {
    let key_str = state
        .agent_pub_key
        .lock()
        .unwrap()
        .clone()
        .ok_or("Agent key not available")?;
    let local_agent = parse_agent_pub_key_string(&key_str)?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(local_agent).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_admin_grant_hash", payload).await?;
    let grant_hash: Option<ActionHash> = result.decode().map_err(|e| e.to_string())?;
    Ok(grant_hash.map(|h| h.to_string()))
}

/// Get all administrators (their agent pubkeys).
#[tauri::command]
pub async fn get_administrators(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<Vec<String>, String> {
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(()).map_err(|e| e.to_string())?;
    let result = call_zome(client, POLLS_ZOME, "get_administrators", payload).await?;
    let admins: Vec<AgentPubKey> = result.decode().map_err(|e| e.to_string())?;

    Ok(admins.iter().map(|pk| pk.to_string()).collect())
}

fn read_lair_backup_fields(data_dir: &Path) -> Option<(String, String, String)> {
    use base64::Engine;
    let passphrase = std::fs::read_to_string(data_dir.join("lair-passphrase")).ok()?;
    let config_yaml =
        std::fs::read_to_string(data_dir.join("lair").join("lair-keystore-config.yaml")).ok()?;
    let store_bytes = std::fs::read(data_dir.join("lair").join("store_file")).ok()?;
    let store_b64 = base64::engine::general_purpose::STANDARD.encode(&store_bytes);
    Some((passphrase, config_yaml, store_b64))
}

// ── Encrypted attachment commands ──────────────────────────────────────
//
// Image bytes are encrypted HOST-SIDE via lair's crypto_box (no 8KB limit),
// once per recipient (each current admin + the uploader). The zome only stores
// the assembled per-recipient ciphers. No x25519 publishing — lair uses the
// agent's Ed25519 key directly (converting to X25519 internally).

/// Mirrors RecipientWrappedKey in the coordinator zome.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct RecipientWrappedKey {
    pub recipient_ed25519: Vec<u8>,
    pub nonce: Vec<u8>,
    pub wrapped_key: Vec<u8>,
}

/// Mirrors StoreAttachmentInput in the coordinator zome.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct StoreAttachmentInput {
    pub finding_action_hash: ActionHash,
    pub image_ciphertext: Vec<u8>,
    pub bulk_nonce: Vec<u8>,
    pub per_recipient: Vec<RecipientWrappedKey>,
    pub sender_ed25519: Vec<u8>,
    pub media_hint: String,
}

/// Extract the 32-byte Ed25519 key from an AgentPubKey (3-byte prefix +
/// 32-byte key + 4-byte DHT location).
fn ed25519_bytes_from_agent(agent: &AgentPubKey) -> [u8; 32] {
    let raw = agent.get_raw_39();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&raw[3..35]);
    bytes
}

/// Create an encrypted attachment on a finding (ring-hybrid). The image is
/// encrypted ONCE host-side with ring (bulk_encrypt, no 8KB limit) under a
/// fresh 32-byte content key. That content key is then wrapped to each
/// recipient (every current admin plus the uploader) via lair's crypto_box --
/// 32 bytes per wrap, comfortably under lair's 8KB frame limit. The cohort is
/// the admin set at upload time; adding an admin later only needs a re-wrap of
/// the 32-byte key, never a re-encrypt of the image.
#[tauri::command]
pub async fn create_encrypted_attachment(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    finding_action_hash: String,
    base64_bytes: String,
    media_hint: String,
) -> Result<String, String> {
    if load_identity_link(&state.data_dir).is_none() {
        return Err("Sign in with Flowsta to add an attachment".to_string());
    }
    let finding_hash = ActionHash::try_from(finding_action_hash)
        .map_err(|e| format!("Invalid finding hash: {:?}", e))?;

    // Decode the base64 image to raw bytes.
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_bytes.as_bytes())
        .map_err(|e| format!("Invalid base64 attachment: {:?}", e))?;

    // The uploader's Ed25519 key (crypto_box sender).
    let sender_ed = get_agent_ed25519_bytes(&state)?;

    // Gather the cohort first (app client only): current admins + uploader.
    let admin_keys: Vec<AgentPubKey> = {
        let client = state.app_client.lock().await;
        let client = client.as_ref().ok_or("Conductor not ready")?;
        let payload = ExternIO::encode(()).map_err(|e| e.to_string())?;
        let result = call_zome(client, POLLS_ZOME, "get_administrators", payload).await?;
        result.decode().map_err(|e| e.to_string())?
    };

    // Build the recipient set as 32-byte Ed25519 keys, deduped, including self.
    let mut recipients: Vec<[u8; 32]> = vec![sender_ed];
    for ak in &admin_keys {
        let eb = ed25519_bytes_from_agent(ak);
        if !recipients.contains(&eb) {
            recipients.push(eb);
        }
    }

    // Ring-hybrid. Encrypt the image ONCE with ring (host-side, no lair, no
    // 8KB limit) under a fresh single-use content key.
    let (content_key, bulk_nonce, image_ciphertext) =
        crate::crypto::bulk_encrypt(&bytes)?;

    // Wrap the 32-byte content key to each recipient via lair. Each wrap is
    // tiny (32 bytes), well under lair's 8KB limit. Hold the lair lock only
    // for the wrapping, release before the store zome call.
    let per_recipient: Vec<RecipientWrappedKey> = {
        let lair = state.lair_client.lock().await;
        let lair = lair.as_ref().ok_or("Lair not connected")?;
        let mut out: Vec<RecipientWrappedKey> = Vec::new();
        for recipient_ed in recipients {
            let (nonce, wrapped_key) =
                crate::crypto::encrypt_to_agent(lair, sender_ed, recipient_ed, &content_key)
                    .await?;
            out.push(RecipientWrappedKey {
                recipient_ed25519: recipient_ed.to_vec(),
                nonce: nonce.to_vec(),
                wrapped_key,
            });
        }
        out
    };

    // Store the assembled attachment via the zome (no crypto in the zome).
    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;
    let input = StoreAttachmentInput {
        finding_action_hash: finding_hash,
        image_ciphertext,
        bulk_nonce: bulk_nonce.to_vec(),
        per_recipient,
        sender_ed25519: sender_ed.to_vec(),
        media_hint,
    };
    let payload = ExternIO::encode(input).map_err(|e| e.to_string())?;
    let result =
        call_zome(client, POLLS_ZOME, "store_encrypted_attachment", payload).await?;
    let hash: ActionHash = result.decode().map_err(|e| e.to_string())?;
    Ok(hash.to_string())
}

/// List attachment action hashes on a finding. The frontend decrypts each via
/// decrypt_attachment.
#[tauri::command]
pub async fn get_finding_attachments(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    finding_action_hash: String,
) -> Result<Vec<String>, String> {
    let finding_hash = ActionHash::try_from(finding_action_hash)
        .map_err(|e| format!("Invalid finding hash: {:?}", e))?;

    let client = state.app_client.lock().await;
    let client = client.as_ref().ok_or("Conductor not ready")?;

    let payload = ExternIO::encode(finding_hash).map_err(|e| e.to_string())?;
    let result =
        call_zome(client, POLLS_ZOME, "get_finding_attachments", payload).await?;
    let records: Vec<Record> = result.decode().map_err(|e| e.to_string())?;

    let hashes = records
        .iter()
        .map(|r| r.signed_action.hashed.hash.to_string())
        .collect();
    Ok(hashes)
}

/// Decrypt an attachment the caller is a recipient of (ring-hybrid). Fetches
/// the attachment, finds the RecipientWrappedKey matching the caller's Ed25519
/// agent key, unwraps the 32-byte content key via lair, then ring-decrypts the
/// single image_ciphertext. Returns the plaintext bytes as base64.
#[tauri::command]
pub async fn decrypt_attachment(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    attachment_action_hash: String,
) -> Result<String, String> {
    let attachment_hash = ActionHash::try_from(attachment_action_hash)
        .map_err(|e| format!("Invalid attachment hash: {:?}", e))?;

    // The caller's Ed25519 key (must match a RecipientWrappedKey).
    let recipient_ed = get_agent_ed25519_bytes(&state)?;

    // Fetch the attachment record from the zome.
    let record: Record = {
        let client = state.app_client.lock().await;
        let client = client.as_ref().ok_or("Conductor not ready")?;
        // get_finding_attachments returns hashes; here we fetch one attachment
        // directly via a small get. Reuse the zome's record fetch by calling
        // get_finding_attachments is per-finding, so instead decode via get.
        let payload = ExternIO::encode(attachment_hash.clone())
            .map_err(|e| e.to_string())?;
        let result =
            call_zome(client, POLLS_ZOME, "get_attachment_record", payload).await?;
        let maybe: Option<Record> = result.decode().map_err(|e| e.to_string())?;
        maybe.ok_or("Attachment not found")?
    };

    // Decode the EncryptedAttachment entry.
    let attachment: EncryptedAttachmentData = decode_entry(&record)?;

    // Find this caller's wrapped content key.
    let mine = attachment
        .per_recipient
        .iter()
        .find(|rc| rc.recipient_ed25519 == recipient_ed.to_vec())
        .ok_or("You are not a recipient of this attachment.")?;

    // Reconstruct sender + nonce, decrypt via lair.
    let mut sender_ed = [0u8; 32];
    if attachment.sender_ed25519.len() != 32 {
        return Err("Malformed attachment sender key".to_string());
    }
    sender_ed.copy_from_slice(&attachment.sender_ed25519);

    let mut nonce = [0u8; 24];
    if mine.nonce.len() != 24 {
        return Err("Malformed attachment key-wrap nonce".to_string());
    }
    nonce.copy_from_slice(&mine.nonce);

    // Stage 1: unwrap the 32-byte content key via lair (small, under 8KB).
    // Drop the lair lock before ring-decrypting (ring needs no lair).
    let content_key_vec = {
        let lair = state.lair_client.lock().await;
        let lair = lair.as_ref().ok_or("Lair not ready")?;
        crate::crypto::decrypt_as_recipient(
            lair, sender_ed, recipient_ed, nonce, &mine.wrapped_key,
        )
        .await?
    };
    if content_key_vec.len() != 32 {
        return Err("Malformed unwrapped content key".to_string());
    }
    let mut content_key = [0u8; 32];
    content_key.copy_from_slice(&content_key_vec);

    // Stage 2: ring-decrypt the single image_ciphertext with the content key.
    if attachment.bulk_nonce.len() != 12 {
        return Err("Malformed bulk nonce".to_string());
    }
    let mut bulk_nonce = [0u8; 12];
    bulk_nonce.copy_from_slice(&attachment.bulk_nonce);
    let plaintext =
        crate::crypto::bulk_decrypt(&content_key, &bulk_nonce, &attachment.image_ciphertext)?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&plaintext))
}

/// Host-side mirror of EncryptedAttachment for decoding.
#[derive(serde::Deserialize, Debug)]
struct EncryptedAttachmentData {
    pub image_ciphertext: Vec<u8>,
    pub bulk_nonce: Vec<u8>,
    pub per_recipient: Vec<RecipientWrappedKey>,
    pub sender_ed25519: Vec<u8>,
    #[allow(dead_code)]
    pub media_hint: String,
    #[allow(dead_code)]
    pub created_at: i64,
}
