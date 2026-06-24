import {
  component$,
  useSignal,
  useVisibleTask$,
  $,
} from "@builder.io/qwik";
import { Link } from "@builder.io/qwik-city";
import { invoke } from "@tauri-apps/api/core";
import {
  addAdministrator,
  signViaVault,
  pubkeyRawB64,
  getAdministrators,
  isAdministrator,
  getArchivedItems,
  unarchiveItem,
  type ItemListItem,
} from "~/lib/holochain";

export default component$(() => {
  const isAdmin = useSignal(false);
  const admins = useSignal<string[]>([]);
  const adminPubkey = useSignal("");
  const submitting = useSignal(false);
  const error = useSignal<string | null>(null);
  const message = useSignal<string | null>(null);

  // Archived scenarios
  const archived = useSignal<ItemListItem[]>([]);
  const archivedLoading = useSignal(true);
  const unarchivingHash = useSignal<string | null>(null);
  const archivedError = useSignal<string | null>(null);
  // Invite generation (Path C): share this network with people you want to join.
  const inviteString = useSignal<string | null>(null);
  const inviteError = useSignal<string | null>(null);
  const inviteCopied = useSignal(false);

  // Load admin status, the admin list, and the archived scenarios.
  useVisibleTask$(async () => {
    try {
      isAdmin.value = await isAdministrator();
      admins.value = await getAdministrators();
    } catch (e) {
      error.value = `Failed to load admin status: ${e}`;
    }
    try {
      archived.value = await getArchivedItems();
    } catch (e) {
      archivedError.value = `Failed to load archived scenarios: ${e}`;
    } finally {
      archivedLoading.value = false;
    }
  });

  const handleAddAdmin = $(async () => {
    error.value = null;
    message.value = null;
    submitting.value = true;
    try {
      const pk = adminPubkey.value.trim();
      if (!pk) {
        error.value = "Admin pubkey cannot be empty";
        submitting.value = false;
        return;
      }
      // The 39-byte raw pubkey is computed in Rust (the frontend keeps no
      // @holochain/client). Sign those bytes with the durable Flowsta key via
      // Vault (user approves in Vault); the integrity zome verifies the
      // signature against the progenitor pubkey burned into the DNA.
      const pkRawB64 = await pubkeyRawB64(pk);
      const { signature } = await signViaVault(
        pkRawB64,
        `Grant admin to ${pk.slice(0, 12)}…`,
      );
      const grantHash = await addAdministrator(pk, signature);
      message.value = `Administrator added. Grant: ${grantHash.slice(0, 16)}…`;
      adminPubkey.value = "";
      admins.value = await getAdministrators();
      submitting.value = false;
    } catch (e) {
      error.value = `Error: ${e}`;
      submitting.value = false;
    }
  });

  // Unarchive: returns the scenario to the board. On success, drop it from the
  // archived list (it's no longer archived) without a full reload.
  const handleUnarchive = $(async (hash: string) => {
    archivedError.value = null;
    unarchivingHash.value = hash;
    try {
      await unarchiveItem(hash);
      archived.value = archived.value.filter((a) => a.hash !== hash);
    } catch (e) {
      archivedError.value = `Failed to unarchive: ${e}`;
    } finally {
      unarchivingHash.value = null;
    }
  });

  // Build a shareable invite for THIS network. Reads the installed DNA's
  // seed + progenitor live (correct across relaunches), then encodes them as
  // fieldnotes://join?seed=...&progenitor=... and copies to the clipboard.
  const generateInvite$ = $(async () => {
    inviteError.value = null;
    inviteCopied.value = false;
    try {
      const info = await invoke<{ network_seed: string; progenitor_pubkey: string | null }>(
        "get_network_info",
      );
      if (!info.progenitor_pubkey) {
        inviteError.value =
          "This network has no progenitor, so there is no admin to invite people on its behalf.";
        return;
      }
      const invite =
        "fieldnotes://join?seed=" +
        encodeURIComponent(info.network_seed) +
        "&progenitor=" +
        encodeURIComponent(info.progenitor_pubkey);
      inviteString.value = invite;
      try {
        await navigator.clipboard.writeText(invite);
        inviteCopied.value = true;
      } catch {
        // Clipboard may be unavailable; the string is shown for manual copy.
      }
    } catch (e) {
      inviteError.value = e instanceof Error ? e.message : String(e);
    }
  });

  return (
    <div class="min-h-screen bg-gray-900 text-gray-100">
      <div class="max-w-2xl mx-auto p-6">
        <Link
          href="/"
          class="text-indigo-400 hover:text-indigo-300 mb-6 inline-block text-sm"
        >
          ← Back to scenarios
        </Link>

        <h1 class="text-3xl font-bold mb-8">Admin</h1>

        <div
          class={`p-3 rounded mb-10 text-sm ${
            isAdmin.value
              ? "bg-green-900/60 border border-green-700"
              : "bg-gray-800 border border-gray-700"
          }`}
        >
          {isAdmin.value
            ? "You are an administrator."
            : "You are not currently an administrator. Become one on the Identity screen."}
        </div>

        {/* ── Scenarios ─────────────────────────────────────────────── */}
        <section class="mb-12">
          <h2 class="text-xl font-bold mb-1">Scenarios</h2>
          <p class="text-sm text-gray-400 mb-4">
            Add scenarios to the board, one at a time or in bulk.
          </p>
          <div class="flex gap-3">
            <Link
              href="/create/"
              class="bg-indigo-600 hover:bg-indigo-500 text-white font-medium px-4 py-2 rounded-lg text-sm"
            >
              Create a scenario
            </Link>
            <Link
              href="/import/"
              class="bg-gray-700 hover:bg-gray-600 text-gray-100 font-medium px-4 py-2 rounded-lg text-sm"
            >
              Import scenarios
            </Link>
          </div>
        </section>
        {/* ── Archived scenarios ────────────────────────────────────── */}
        <section class="mb-12">
          <h2 class="text-xl font-bold mb-1">Archived scenarios</h2>
          <p class="text-sm text-gray-400 mb-4">
            Hidden from the board, but preserved with their responses and
            findings. Unarchive to return one to the board.
          </p>

          {archivedError.value && (
            <div class="bg-red-900 border border-red-700 p-3 rounded mb-4 text-sm">
              {archivedError.value}
            </div>
          )}

          {archivedLoading.value ? (
            <p class="text-gray-500 text-sm">Loading…</p>
          ) : archived.value.length === 0 ? (
            <p class="text-gray-500 text-sm">No archived scenarios.</p>
          ) : (
            <ul class="space-y-2">
              {archived.value.map((a) => (
                <li
                  key={a.hash}
                  class="bg-gray-800 border border-gray-700 rounded-lg p-3 flex items-center justify-between gap-3"
                >
                  <div class="min-w-0">
                    <p class="text-sm text-gray-200 truncate">{a.item.title}</p>
                    <p class="text-xs text-gray-500 truncate">
                      {a.item.section}
                      {a.item.campaign ? ` · ${a.item.campaign}` : ""}
                    </p>
                  </div>
                  <button
                    onClick$={() => handleUnarchive(a.hash)}
                    disabled={unarchivingHash.value === a.hash}
                    class="shrink-0 bg-emerald-700 hover:bg-emerald-600 disabled:opacity-50 text-white font-medium px-3 py-1.5 rounded-full text-xs"
                  >
                    {unarchivingHash.value === a.hash
                      ? "Unarchiving…"
                      : "Unarchive"}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        {/* ── Invite people to this network ─────────────────────────── */}
        <section class="mb-12">
          <h2 class="text-xl font-bold mb-1">Invite people to this network</h2>
          <p class="text-sm text-gray-400 mb-4">
            Share this invite with anyone you want to join your network. They
            paste it on their first run to join as a member.
          </p>
          <button
            type="button"
            onClick$={generateInvite$}
            class="bg-indigo-600 hover:bg-indigo-500 text-white font-medium px-4 py-2 rounded-lg text-sm"
          >
            Generate invite
          </button>
          {inviteError.value && (
            <p class="text-red-400 text-sm mt-3">{inviteError.value}</p>
          )}
          {inviteString.value && (
            <div class="mt-4">
              <input
                type="text"
                readOnly
                value={inviteString.value}
                onClick$={(_, el) => el.select()}
                class="w-full bg-gray-800 border border-gray-700 rounded-md px-3 py-2 text-xs text-gray-200 font-mono"
              />
              <p class="text-xs text-gray-500 mt-1">
                {inviteCopied.value
                  ? "Copied to clipboard."
                  : "Click to select, then copy."}
              </p>
            </div>
          )}
        </section>

        {/* ── Administrators ────────────────────────────────────────── */}
        <section class="mb-12">
          <h2 class="text-xl font-bold mb-4">Current administrators</h2>
          {admins.value.length === 0 ? (
            <p class="text-gray-500 text-sm">No administrators yet.</p>
          ) : (
            <ul class="space-y-2">
              {admins.value.map((admin) => (
                <li
                  key={admin}
                  class="bg-gray-800 p-3 rounded font-mono text-xs break-all text-gray-300"
                >
                  {admin}
                </li>
              ))}
            </ul>
          )}
        </section>

        {/* ── Add administrator ─────────────────────────────────────── */}
        <section>
          <h2 class="text-xl font-bold mb-1">Add an administrator</h2>
          <p class="text-sm text-gray-400 mb-4">
            Grant administrator authority to another agent by their Fieldnotes
            agent key. The grant is re-issued after any breaking change.
          </p>
          {error.value && (
            <div class="bg-red-900 border border-red-700 p-3 rounded mb-4 text-sm">
              {error.value}
            </div>
          )}
          {message.value && (
            <div class="bg-green-900 border border-green-700 p-3 rounded mb-4 text-sm">
              {message.value}
            </div>
          )}
          <div class="space-y-3">
            <input
              type="text"
              placeholder="Agent key (uhCAk…)"
              value={adminPubkey.value}
              onInput$={(_, el) => (adminPubkey.value = el.value)}
              class="w-full bg-gray-800 border border-gray-700 p-3 rounded text-gray-100 placeholder-gray-500 font-mono text-sm"
            />
            <button
              onClick$={handleAddAdmin}
              disabled={submitting.value}
              class="bg-indigo-600 hover:bg-indigo-500 disabled:bg-gray-600 text-white font-medium py-2 px-4 rounded-lg text-sm"
            >
              {submitting.value ? "Adding…" : "Add administrator"}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
});
