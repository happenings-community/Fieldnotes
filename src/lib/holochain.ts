/**
 * Holochain zome call helpers for Fieldnotes.
 *
 * Thin wrappers around Tauri invoke() — all zome calls go through the Rust
 * backend. No @holochain/client in the frontend.
 *
 * ## For forking developers
 *
 * This file has four sections:
 *   1. **App types + functions** (top) — Poll, Vote, Flag types and their
 *      invoke() wrappers. Replace these with your own data model.
 *   2. **Identity linking** — Flowsta integration. Keep as-is.
 *   3. **Flagging** — Community moderation. Keep or adapt.
 *   4. **Migration status** — DNA version upgrade tracking. Keep as-is.
 *
 * Each function is a one-liner that calls the matching Tauri command in
 * `src-tauri/src/commands.rs`. Add/remove functions as you add/remove commands.
 */

import { invoke } from "@tauri-apps/api/core";

// ── App-specific types (replace with your data model) ─────────────────

export type PollType = "Anonymous" | "Public";

export interface Poll {
  title: string;
  description: string;
  options: string[];
  created_at: number;
  closes_at: number | null;
  /** "Anonymous" or "Public". Null for pre-v1.2 polls (treated as Anonymous). */
  poll_type: PollType | null;
}

export interface PollListItem {
  hash: string;
  poll: Poll;
  author: string;
  /** Which DHT this poll lives on. Pass back to castVote and getPollVotes. */
  dna_version: "1.0" | "1.1" | "1.2" | "1.3";
}

export interface PollDetail {
  poll: Poll;
  author: string;
  /** Which DHT this poll lives on. Pass back to castVote and getPollVotes. */
  dna_version: "1.0" | "1.1" | "1.2" | "1.3";
}

export interface VoteData {
  vote: { hash: string; poll_action_hash: string; option_index: number };
  author: string;
  /** Set on public v1.2 polls. Null otherwise. */
  display_name: string | null;
  /** Set on public v1.2 polls. Null otherwise. */
  profile_picture: string | null;
}

// ── App-specific operations (replace with your invoke() wrappers) ─────

export async function createPoll(input: {
  title: string;
  description: string;
  options: string[];
  closes_at: number | null;
  poll_type: PollType;
}): Promise<string> {
  // Tauri v2 maps camelCase from JS → snake_case in Rust, so we must send
  // camelCase keys even though the TypeScript interface uses snake_case.
  return invoke<string>("create_poll", {
    title: input.title,
    description: input.description,
    options: input.options,
    closesAt: input.closes_at,
    pollType: input.poll_type,
  });
}

export async function getPoll(actionHash: string): Promise<PollDetail | null> {
  return invoke<PollDetail | null>("get_poll", { actionHash });
}

export async function getAllPolls(): Promise<PollListItem[]> {
  return invoke<PollListItem[]>("get_all_polls");
}

export async function deletePoll(actionHash: string): Promise<string> {
  return invoke<string>("delete_poll", { actionHash });
}

export async function castVote(
  pollActionHash: string,
  optionIndex: number,
  dnaVersion: "1.0" | "1.1" | "1.2",
  pollType?: PollType,
): Promise<string> {
  return invoke<string>("cast_vote", {
    pollActionHash,
    optionIndex,
    dnaVersion,
    pollType: pollType ?? null,
  });
}

export async function getPollVotes(
  pollActionHash: string,
  dnaVersion: "1.0" | "1.1" | "1.2",
): Promise<VoteData[]> {
  return invoke<VoteData[]>("get_poll_votes", {
    pollActionHash,
    dnaVersion,
  });
}

// ── Fieldnotes types (Item / Response / Finding) ──────────────────────
//
// Replaces the poll data model. Return shapes are snake_case (serde field
// names from Rust). When SENDING args, Tauri maps camelCase -> snake_case
// for top-level command args (so createItem sends `lookFor`), but NOT for
// objects nested inside an array (see importItems).

export type ItemKind = "Scenario" | "Feedback";
export type Verdict = "Pass" | "Fail" | "Partial" | "Skip";

export interface Item {
  kind: ItemKind;
  campaign: string;
  section: string;
  title: string;
  instructions: string;
  look_for: string;
  order: number;
  created_at: number;
}

export interface ItemListItem {
  hash: string;
  item: Item;
  author: string;
}

export interface ItemDetail {
  item: Item;
  author: string;
}

export interface ResponseData {
  hash: string;
  item_action_hash: string;
  verdict: Verdict;
  author: string;
  created_at: number;
}

export interface FindingData {
  hash: string;
  item_action_hash: string;
  text: string;
  author: string;
  created_at: number;
}

export interface CreateItemInput {
  kind: ItemKind;
  campaign: string;
  section: string;
  title: string;
  instructions: string;
  look_for: string;
  order: number;
  admin_grant_action_hash?: string | null;
}

// ── Scenarios ─────────────────────────────────────────────────────────

export async function getAllItems(): Promise<ItemListItem[]> {
  return invoke<ItemListItem[]>("get_all_items");
}

export async function getArchivedItems(): Promise<ItemListItem[]> {
  return invoke<ItemListItem[]>("get_archived_items");
}

export async function getItem(actionHash: string): Promise<ItemDetail | null> {
  return invoke<ItemDetail | null>("get_item", { actionHash });
}

export async function archiveItem(actionHash: string): Promise<string> {
  return invoke<string>("archive_item", { actionHash });
}

export async function unarchiveItem(actionHash: string): Promise<string> {
  return invoke<string>("unarchive_item", { actionHash });
}

// ── Encrypted attachments ──────────────────────────────────────────────
// Cohort-scoped encrypted attachments on findings. Encryption happens
// host-side (ring for the image, lair to wrap the content key per admin);
// the frontend sends/receives plaintext bytes (as base64) and hashes.

/// Encrypt `bytes` (base64) and store as an attachment on a finding. The host
/// encrypts the image once with ring and wraps the content key to each current
/// admin plus the uploader. Returns the attachment action hash.
export async function createEncryptedAttachment(
  findingActionHash: string,
  base64Bytes: string,
  mediaHint: string,
): Promise<string> {
  return invoke<string>("create_encrypted_attachment", {
    findingActionHash,
    base64Bytes,
    mediaHint,
  });
}

/// List attachment action hashes on a finding (no plaintext).
export async function getFindingAttachments(
  findingActionHash: string,
): Promise<string[]> {
  return invoke<string[]>("get_finding_attachments", { findingActionHash });
}

/// Decrypt an attachment the caller is in the cohort for. Returns the
/// plaintext bytes as base64 (the caller turns it back into an image/blob).
export async function decryptAttachment(
  attachmentActionHash: string,
): Promise<string> {
  return invoke<string>("decrypt_attachment", { attachmentActionHash });
}

export async function createItem(input: CreateItemInput): Promise<string> {
  // Top-level args: Tauri maps camelCase -> snake_case, so send `lookFor`.
  return invoke<string>("create_item", {
    kind: input.kind,
    campaign: input.campaign,
    section: input.section,
    title: input.title,
    instructions: input.instructions,
    lookFor: input.look_for,
    order: input.order,
    adminGrantActionHash: input.admin_grant_action_hash ?? null,
  });
}

export async function getAdminGrantHash(): Promise<string | null> {
  const result = await invoke("get_admin_grant_hash", {});
  return result as string | null;
}

export async function importItems(items: CreateItemInput[]): Promise<number> {
  // Objects nested in the array keep snake_case keys (no camelCase mapping
  // inside arrays). Verify empirically when the import screen is wired.
  return invoke<number>("import_items", { items });
}

// ── Responses (verdicts) ──────────────────────────────────────────────

export async function respond(
  itemActionHash: string,
  verdict: Verdict,
): Promise<string> {
  return invoke<string>("respond", { itemActionHash, verdict });
}

export async function getItemResponses(
  itemActionHash: string,
): Promise<ResponseData[]> {
  return invoke<ResponseData[]>("get_item_responses", { itemActionHash });
}

// ── Findings ──────────────────────────────────────────────────────────

export async function createFinding(
  itemActionHash: string,
  text: string,
): Promise<string> {
  return invoke<string>("create_finding", { itemActionHash, text });
}

export async function getItemFindings(
  itemActionHash: string,
): Promise<FindingData[]> {
  return invoke<FindingData[]>("get_item_findings", { itemActionHash });
}

// Runtime environment string for the "Same here" corroboration stamp,
// e.g. "macOS 26.5.1 (arm64)". Read from the host (the webview UA can't
// supply a real macOS version). Takes no args, so no camelCase mapping.
export async function appEnvironment(): Promise<string> {
  return invoke<string>("app_environment");
}

// ── Profile cache (Flowsta infrastructure — keep as-is) ───────────────
//
// Caches the user's display name and profile picture locally so the app
// works without the Flowsta Vault running. The Vault is only needed for
// the initial identity linking ceremony. See layout.tsx for the load/save flow.

export interface CachedProfile {
  display_name: string | null;
  profile_picture: string | null;
}

export async function getCachedProfile(): Promise<CachedProfile | null> {
  return invoke<CachedProfile | null>("get_cached_profile");
}

export async function saveProfileCache(
  displayName: string | null,
  profilePicture: string | null,
): Promise<void> {
  return invoke<void>("save_profile_cache", {
    displayName,
    profilePicture,
  });
}

// ── Identity linking (Flowsta infrastructure — keep as-is) ────────────

export interface IdentityLinkData {
  vault_agent_pub_key: string;
  entry_action_hash: string;
  linked_at: number;
}

export async function commitIdentityLink(
  vaultAgentPubKey: string,
  vaultSignature: string,
): Promise<string> {
  return invoke<string>("commit_identity_link", {
    vaultAgentPubKey,
    vaultSignature,
  });
}

export async function getLinkedAgents(
  agentPubKey: string,
): Promise<string[]> {
  return invoke<string[]>("get_linked_agents", {
    agentPubKey,
  });
}

export async function getIdentityLink(): Promise<IdentityLinkData | null> {
  return invoke<IdentityLinkData | null>("get_identity_link");
}

/**
 * The set of Holochain agent keys that all belong to THIS user — used to
 * recognise the user's own polls/votes/flags regardless of which device or
 * install authored them.
 *
 * Why a set, not a single key: Fieldnotes generates a fresh conductor agent
 * key on every install. The user's stable identity is their Flowsta Vault
 * agent; each install links its local agent to that Vault agent via an
 * IsSamePerson attestation. `get_linked_agents(vaultAgent)` therefore returns
 * every Fieldnotes agent the user has ever linked (this is a designed-in query
 * — the agent-linking zome indexes the link from the Vault agent's pubkey too).
 *
 * IMPORTANT: this is for RECOGNITION (read) only. Mutating an entry
 * (delete a poll, remove a flag) is still bound to the CURRENT local agent —
 * Holochain only lets the original author update/delete, so a different linked
 * agent cannot. Use the local agent directly for those gates, not this set.
 *
 * Best-effort: if the user has never linked (fresh, not signed in) or the
 * Vault link isn't available, the set is just the local agent.
 */
export async function loadMyAgentSet(
  localAgent: string | null,
): Promise<Set<string>> {
  try {
    // Single Rust round-trip: local agent ∪ agents linked to our Vault
    // identity. The whole lookup (and its result) is logged to fieldnotes.log
    // by the `get_my_agent_set` command, so recognition is verifiable.
    const agents = await invoke<string[]>("get_my_agent_set", { localAgent });
    return new Set(agents);
  } catch {
    // Conductor not ready / offline — fall back to the local agent only.
    return new Set(localAgent ? [localAgent] : []);
  }
}

export async function revokeIdentityLink(): Promise<void> {
  return invoke<void>("revoke_identity_link");
}

// ── Flagging (community moderation — keep or adapt) ───────────────────

export type FlagReason = "Spam" | "Misleading" | "OffTopic" | "Inappropriate";

export interface FlagData {
  hash: string;
  flag: { poll_action_hash: string; reason: string; created_at: number };
  author: string;
}

export async function flagPoll(
  pollActionHash: string,
  reason: FlagReason,
): Promise<string> {
  return invoke<string>("flag_poll", { pollActionHash, reason });
}

export async function getPollFlags(
  pollActionHash: string,
): Promise<FlagData[]> {
  return invoke<FlagData[]>("get_poll_flags", { pollActionHash });
}

export async function removeFlag(flagActionHash: string): Promise<string> {
  return invoke<string>("remove_flag", { flagActionHash });
}

export async function getFlagThreshold(): Promise<number> {
  return invoke<number>("get_flag_threshold");
}

// ── Migration status (infrastructure — keep as-is) ────────────────────

export interface MigrationState {
  status: "NotStarted" | "InProgress" | "Complete" | { Error: string };
  polls_migrated: { old_hash: string; new_hash: string; title: string }[];
  votes_pending: {
    v1_0_poll_hash: string;
    option_index: number;
    poll_title: string;
    retry_count: number;
  }[];
  votes_migrated: {
    old_poll_hash: string;
    new_poll_hash: string;
    option_index: number;
  }[];
}

export async function getMigrationStatus(): Promise<MigrationState> {
  return invoke<MigrationState>("get_migration_status");
}

export async function abandonPendingVotes(): Promise<void> {
  return invoke<void>("abandon_pending_votes");
}

// ── Encrypted entries (v1.3) ──────────────────────────────────────────

export interface DraftPollItem {
  hash: string;
  title: string;
  description: string;
  options: string[];
  closes_at: number | null;
  poll_type: string;
  created_at: number;
}

export async function saveVoteRationale(
  voteActionHash: string,
  rationaleText: string,
): Promise<string> {
  return invoke<string>("save_vote_rationale", {
    voteActionHash,
    rationaleText,
  });
}

export async function getVoteRationale(
  voteActionHash: string,
): Promise<string | null> {
  return invoke<string | null>("get_vote_rationale", { voteActionHash });
}

export async function saveDraftPoll(input: {
  title: string;
  description: string;
  options: string[];
  closes_at: number | null;
  poll_type: PollType;
}): Promise<string> {
  return invoke<string>("save_draft_poll", {
    title: input.title,
    description: input.description,
    options: input.options,
    closesAt: input.closes_at,
    pollType: input.poll_type,
  });
}

export async function getMyDrafts(): Promise<DraftPollItem[]> {
  return invoke<DraftPollItem[]>("get_my_drafts");
}

export async function publishDraft(draftActionHash: string): Promise<string> {
  return invoke<string>("publish_draft", { draftActionHash });
}

export async function deleteDraft(draftActionHash: string): Promise<string> {
  return invoke<string>("delete_draft", { draftActionHash });
}

/**
 * Ask the user's Flowsta Vault to sign raw bytes with their DURABLE device
 * identity key (NOT the volatile cell agent). Used to produce a progenitor
 * signature over an admin pubkey, which validate_admin_grant verifies against
 * the progenitor key burned into the DNA.
 *
 * Vault gates /sign to linked apps (we link via linkFlowstaIdentity), and
 * prompts the user to approve each request. Returns the base64 signature and
 * the signer's agent pubkey (the durable device key = the progenitor key).
 */
export async function signViaVault(
  bytesB64: string,
  reason: string,
): Promise<{ signature: number[]; agentPubKey: string }> {
  const resp = await fetch("http://127.0.0.1:27777/sign", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ type: "bytes", bytes: bytesB64, reason }),
    signal: AbortSignal.timeout(60000), // user must approve in Vault
  });
  if (!resp.ok) {
    let detail = "";
    try {
      const e = await resp.json();
      detail = e.description || e.error || "";
    } catch {
      // non-JSON error body
    }
    throw new Error(`Vault /sign failed (${resp.status}): ${detail}`);
  }
  const out = await resp.json();
  if (!out.success || !out.signature) {
    throw new Error("Vault /sign returned no signature");
  }
  // Vault returns the signature base64-encoded; decode to a byte array so it
  // can be passed straight to add_administrator (Rust Vec<u8>).
  const sigBytes = Array.from(atob(out.signature), (c) => c.charCodeAt(0));
  return { signature: sigBytes, agentPubKey: out.agent_pub_key };
}

/**
 * Get the 39 raw bytes of an agent pubkey string, base64-encoded, computed in
 * Rust (the frontend keeps no @holochain/client). These are the exact bytes
 * signed for an AdminGrant and verified by the integrity zome.
 */
export async function pubkeyRawB64(pubkeyStr: string): Promise<string> {
  return invoke<string>("pubkey_raw_b64", { pubkeyStr });
}

export async function addAdministrator(
  adminPubkeyStr: string,
  progenitorSignature: number[]
): Promise<string> {
  const result = await invoke("add_administrator", {
    adminPubkeyStr,
    progenitorSignature,
  });
  return result as string;
}

export async function isAdministrator(): Promise<boolean> {
  const result = await invoke("is_administrator", {});
  return result as boolean;
}

export async function getAdministrators(): Promise<string[]> {
  const result = await invoke("get_administrators", {});
  return result as string[];
}
