import {
  component$,
  useContext,
  useSignal,
  useVisibleTask$,
  $,
} from "@builder.io/qwik";
import { Link, useNavigate } from "@builder.io/qwik-city";

// Why hash-based instead of /poll/[id]/?
// The Qwik static adapter only pre-renders routes whose param values are
// enumerated up-front. Scenario action hashes aren't, so navigating to
// /poll/<unknown-hash>/ fails: the per-route q-data.json doesn't exist, the
// client router silently aborts the render, and the click looks like a no-op.
// Using /poll/#<hash> sidesteps it — /poll/ is a fully static route, the hash
// is browser-only (never sent to the server-shaped asset resolver), and
// switching scenarios is just a hash change.
import { linkedContext } from "~/lib/context";
import { formatInvokeError } from "~/lib/errors";
import { invoke } from "@tauri-apps/api/core";
import {
  getItem,
  getItemResponses,
  respond,
  getItemFindings,
  createFinding,
  appEnvironment,
  loadMyAgentSet,
  archiveItem,
  isAdministrator,
  type Item,
  type ResponseData,
  type FindingData,
  type Verdict,
} from "~/lib/holochain";

// The four verdicts, in display order, paired with the Tailwind accent each
// uses when it is the viewer's current selection. Order here is the order the
// buttons render in.
const VERDICTS: { value: Verdict; label: string; accent: string }[] = [
  {
    value: "Pass",
    label: "Pass",
    accent: "bg-emerald-600 border-emerald-500 text-white",
  },
  {
    value: "Fail",
    label: "Fail",
    accent: "bg-red-600 border-red-500 text-white",
  },
  {
    value: "Partial",
    label: "Partial",
    accent: "bg-amber-600 border-amber-500 text-white",
  },
  {
    value: "Skip",
    label: "Skip",
    accent: "bg-gray-600 border-gray-500 text-white",
  },
];

// Short, stable label for an agent pubkey until display-name resolution exists
// (the agent-key → Flowsta display-name mapping is a later identity layer; the
// zome deliberately doesn't store names on Finding/Response). First 8 chars is
// enough to tell two testers apart at a glance.
function shortAgent(pubkey: string): string {
  return pubkey.length > 8 ? `${pubkey.slice(0, 8)}…` : pubkey;
}

// Render a finding's millisecond timestamp as a local date-time. created_at is
// the snake_case field from the host (serde), in ms.
function formatTime(ms: number): string {
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return "";
  }
}

export default component$(() => {
  const linked = useContext(linkedContext);

  // The scenario's action hash is read from window.location.hash at client
  // visible-task time (the hash is never present on the server side).
  const itemHash = useSignal<string>("");
  const item = useSignal<Item | null>(null);
  const author = useSignal<string | null>(null);
  const loading = useSignal(true);
  const error = useSignal<string | null>(null);

  // ── Verdict state ──
  // myAgent: this install's local agent pubkey. The viewer's CURRENT verdict is
  // matched against this single key (a verdict write is author-bound to the
  // local agent, mirroring castVote in the pre-refactor code).
  const myAgent = useSignal<string | null>(null);
  const isAdmin = useSignal(false);
  const archiving = useSignal(false);
  const archiveError = useSignal<string | null>(null);
  const archiveConfirm = useSignal(false);
  const nav = useNavigate();
  // myAgentSet: every agent key belonging to this user across installs
  // (recognition only — see loadMyAgentSet). Used to recognise whether the user
  // has ALREADY responded from some other linked install, so a reinstalled user
  // isn't shown as never-having-responded.
  //
  // KNOWN LATER LAYER (not this phase): respond() dedupes by single author at
  // the zome level, so responding here when another linked install already
  // responded creates a SECOND response under the local key rather than
  // superseding the first. Fixing that is a coordinator-level change (dedupe
  // against the author-set, not one author) — a later backend pass, not a
  // frontend one. The cross-device recognition pattern to build on is in commit
  // 8919956 ("Recognise a user's polls and votes across devices via Flowsta
  // identity"). The set-match here is display-only and does not fix the write.
  const myAgentSet = useSignal<Set<string>>(new Set());
  const responses = useSignal<ResponseData[]>([]);
  const submitting = useSignal(false);
  const verdictError = useSignal<string | null>(null);

  // ── Findings state ──
  // Findings are free-text observations on a scenario: append-only, many per
  // agent, PLAINTEXT on the DHT for v0.0.1 (cohort-visible). No "replace mine"
  // logic (unlike verdicts) and no tally — just the thread, newest last.
  //
  // KNOWN LATER LAYERS (not this phase): cohort encryption of findings (the
  // Model B evidence layer — private screenshots/logs encrypted to
  // {admins + uploader}) and the secret-scan soft-redirect (regex for
  // PEM/ghp_/AKIA/JWT → "attach as a private log instead"). Both come after the
  // tool is usable; this phase is plaintext only.
  const findings = useSignal<FindingData[]>([]);
  const findingInput = useSignal("");
  const findingSubmitting = useSignal(false);
  const findingError = useSignal<string | null>(null);

  useVisibleTask$(async () => {
    const hash = window.location.hash.startsWith("#")
      ? window.location.hash.slice(1)
      : window.location.hash;
    itemHash.value = hash;
    if (!hash) {
      error.value = "Scenario not found";
      loading.value = false;
      return;
    }
    try {
      const status = await invoke<{ agent_pub_key: string | null }>(
        "get_app_status",
      );
      myAgent.value = status.agent_pub_key;
      myAgentSet.value = await loadMyAgentSet(status.agent_pub_key);

      const result = await getItem(hash);
      if (!result) {
        error.value = "Scenario not found";
        return;
      }
      item.value = result.item;
      author.value = result.author;

      const [responsesResult, findingsResult, adminResult] = await Promise.all([
        getItemResponses(hash),
        getItemFindings(hash),
        isAdministrator(),
      ]);
      responses.value = responsesResult;
      findings.value = findingsResult;
      isAdmin.value = adminResult;
    } catch (e: any) {
      error.value = formatInvokeError(e, "Failed to load scenario");
    } finally {
      loading.value = false;
    }
  });

  // Record (or change) the viewer's verdict, then reload the response list so
  // the tally and the viewer's own selection reflect the write. Mirrors the
  // write-then-reload shape of the old submitVote.
  const submitVerdict = $(async (verdict: Verdict) => {
    if (!itemHash.value) return;
    verdictError.value = null;
    submitting.value = true;
    try {
      await respond(itemHash.value, verdict);
      responses.value = await getItemResponses(itemHash.value);
    } catch (e: any) {
      verdictError.value = formatInvokeError(e, "Failed to record verdict");
    } finally {
      submitting.value = false;
    }
  });

  // Archive this scenario (admin-only). On success the item drops out of
  // get_all_items, so we return to the board where it will no longer appear.
  const archive = $(async () => {
    if (!itemHash.value) return;
    archiveError.value = null;
    archiving.value = true;
    try {
      await archiveItem(itemHash.value);
      nav("/");
    } catch (e: any) {
      archiveError.value = formatInvokeError(e, "Failed to archive scenario");
      archiving.value = false;
    }
  });

  // Post a finding, clear the box, reload the thread. Append-only, so reload
  // simply re-reads the full list (newest will be at the end by created_at).
  const submitFinding = $(async () => {
    const text = findingInput.value.trim();
    if (!text || !itemHash.value) return;
    findingError.value = null;
    findingSubmitting.value = true;
    try {
      await createFinding(itemHash.value, text);
      findingInput.value = "";
      findings.value = await getItemFindings(itemHash.value);
    } catch (e: any) {
      findingError.value = formatInvokeError(e, "Failed to add finding");
    } finally {
      findingSubmitting.value = false;
    }
  });

  // "Same here": one-tap corroboration on an emergent issue. Stamps the
  // host environment (read at click time, so it can't go stale) and records
  // it as a +1 finding, then reloads the thread. Same write-then-reload
  // shape as submitFinding. Minimal by design - no count, no dedupe (see
  // the file header note); a +1 is just a finding.
  const submitSameHere = $(async () => {
    if (!itemHash.value) return;
    findingError.value = null;
    findingSubmitting.value = true;
    try {
      const env = await appEnvironment();
      await createFinding(itemHash.value, "+1 · " + env);
      findings.value = await getItemFindings(itemHash.value);
    } catch (e: any) {
      findingError.value = formatInvokeError(e, "Failed to record \"Same here\"");
    } finally {
      findingSubmitting.value = false;
    }
  });

  if (loading.value) {
    return <div class="text-gray-400">Loading scenario...</div>;
  }

  if (error.value) {
    return (
      <div class="max-w-md mx-auto mt-12">
        <div class="bg-red-900/20 border border-red-800/40 rounded-lg p-5">
          <p class="text-red-300 text-sm font-medium mb-1">
            Couldn't load scenario
          </p>
          <p class="text-red-400/70 text-xs mb-3">{error.value}</p>
          <a
            href={`/poll/#${itemHash.value}`}
            class="text-xs bg-red-800/40 hover:bg-red-800/60 text-red-300 px-3 py-1.5 rounded-full font-medium transition-colors inline-block"
          >
            Try again
          </a>
        </div>
      </div>
    );
  }

  if (!item.value) return null;

  const it = item.value;

  // The viewer's CURRENT verdict: the response authored by this local agent, if
  // any. Single-author match (not the set) because the write is author-bound.
  const myVerdict =
    responses.value.find((r) => r.author === myAgent.value)?.verdict ?? null;

  // Per-verdict tally across all responses on this scenario.
  const counts: Record<Verdict, number> = {
    Pass: 0,
    Fail: 0,
    Partial: 0,
    Skip: 0,
  };
  for (const r of responses.value) counts[r.verdict] += 1;
  const totalResponses = responses.value.length;

  // Findings oldest-first so the thread reads top-to-bottom chronologically.
  const sortedFindings = [...findings.value].sort(
    (a, b) => a.created_at - b.created_at,
  );

  return (
    <div class="max-w-2xl mx-auto">
      <Link
        href="/"
        class="text-sm text-gray-500 hover:text-gray-300 inline-flex items-center gap-1 mb-5"
      >
        <svg
          class="w-4 h-4"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width={2}
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M15 19l-7-7 7-7"
          />
        </svg>
        Back to scenarios
      </Link>

      <div class="mb-6">
        <div class="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-2">
          {it.section}
          {it.campaign && (
            <span class="text-gray-600 normal-case font-normal">
              {" "}
              · {it.campaign}
            </span>
          )}
        </div>
        <h1 class="text-2xl font-bold">{it.title}</h1>
        {isAdmin.value && (
          <div class="mt-3">
            {!archiveConfirm.value ? (
              <button
                onClick$={() => (archiveConfirm.value = true)}
                class="text-xs text-gray-500 hover:text-gray-300 underline underline-offset-2"
              >
                Archive this scenario
              </button>
            ) : (
              <div class="flex items-center gap-3 text-xs">
                <span class="text-gray-400">
                  Archive this scenario? It will be hidden from the board.
                </span>
                <button
                  onClick$={archive}
                  disabled={archiving.value}
                  class="px-2 py-1 rounded bg-amber-700 hover:bg-amber-600 text-white disabled:opacity-50"
                >
                  {archiving.value ? "Archiving…" : "Confirm"}
                </button>
                <button
                  onClick$={() => (archiveConfirm.value = false)}
                  disabled={archiving.value}
                  class="px-2 py-1 rounded border border-gray-700 text-gray-400 hover:text-gray-200 disabled:opacity-50"
                >
                  Cancel
                </button>
              </div>
            )}
            {archiveError.value && (
              <p class="mt-2 text-xs text-red-400">{archiveError.value}</p>
            )}
          </div>
        )}
      </div>

      <div class="space-y-5">
        {it.instructions.trim() && (
          <div class="bg-gray-900 border border-gray-800 rounded-lg p-5">
            <h2 class="text-sm font-medium text-gray-300 mb-2">
              {it.kind === "Feedback" ? "What happened" : "What to do"}
            </h2>
            <p class="text-sm text-gray-200 whitespace-pre-wrap leading-relaxed">
              {it.instructions}
            </p>
          </div>
        )}

        {it.look_for.trim() && (
          <div class="bg-indigo-950/30 border border-indigo-900/50 rounded-lg p-5">
            <h2 class="text-sm font-medium text-indigo-300 mb-2">Look for</h2>
            <p class="text-sm text-gray-200 whitespace-pre-wrap leading-relaxed">
              {it.look_for}
            </p>
          </div>
        )}

        {it.kind !== "Feedback" &&
          !it.instructions.trim() &&
          !it.look_for.trim() && (
            <p class="text-gray-500 text-sm">
              No instructions recorded for this scenario yet.
            </p>
          )}

        {/* ── Verdict control (scenarios only; an emergent issue isn't
            Pass/Fail/Partial/Skip-shaped, so it's hidden for Feedback) ── */}
        {it.kind !== "Feedback" && (
          <div class="bg-gray-900 border border-gray-800 rounded-lg p-5">
          <div class="flex items-baseline justify-between mb-3">
            <h2 class="text-sm font-medium text-gray-300">Your verdict</h2>
            <span class="text-xs text-gray-500">
              {totalResponses} response{totalResponses !== 1 ? "s" : ""}
            </span>
          </div>

          {linked.value ? (
            <>
              <div class="grid grid-cols-2 sm:grid-cols-4 gap-2">
                {VERDICTS.map((v) => {
                  const isMine = myVerdict === v.value;
                  return (
                    <button
                      key={v.value}
                      type="button"
                      disabled={submitting.value}
                      onClick$={() => submitVerdict(v.value)}
                      class={[
                        "flex flex-col items-center justify-center rounded-lg border px-3 py-2.5 text-sm font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                        isMine
                          ? v.accent
                          : "bg-gray-800 border-gray-700 text-gray-300 hover:bg-gray-750 hover:border-gray-600",
                      ].join(" ")}
                    >
                      <span>{v.label}</span>
                      <span
                        class={[
                          "text-xs mt-0.5",
                          isMine ? "text-white/80" : "text-gray-500",
                        ].join(" ")}
                      >
                        {counts[v.value]}
                      </span>
                    </button>
                  );
                })}
              </div>

              {myVerdict && (
                <p class="text-xs text-gray-500 mt-3">
                  Your current verdict is{" "}
                  <span class="text-gray-300 font-medium">{myVerdict}</span>.
                  Tap another to change it.
                </p>
              )}

              {verdictError.value && (
                <p class="text-xs text-red-400 mt-3">{verdictError.value}</p>
              )}
            </>
          ) : (
            <p class="text-sm text-gray-500">
              Sign in with Flowsta to record a verdict on this scenario.
            </p>
          )}
          </div>
        )}

        {/* ── Findings thread ── */}
        <div class="bg-gray-900 border border-gray-800 rounded-lg p-5">
          <div class="flex items-baseline justify-between mb-3">
            <h2 class="text-sm font-medium text-gray-300">Findings</h2>
            <span class="text-xs text-gray-500">
              {sortedFindings.length} finding
              {sortedFindings.length !== 1 ? "s" : ""}
            </span>
          </div>

          {/* "Same here" - one-tap corroboration on an emergent issue,
              records an env-stamped +1 finding. Feedback-only, sign-in-gated. */}
          {it.kind === "Feedback" && linked.value && (
            <div class="mb-4">
              <button
                type="button"
                disabled={findingSubmitting.value}
                onClick$={submitSameHere}
                class="w-full sm:w-auto text-sm bg-amber-600 hover:bg-amber-500 disabled:opacity-50 disabled:cursor-not-allowed text-white px-4 py-2 rounded-lg font-medium transition-colors"
              >
                Same here — I hit this too
              </button>
              <p class="text-xs text-gray-500 mt-1.5">
                Records a +1 with your OS and version, so the team can see
                how widely this hits.
              </p>
            </div>
          )}

          {sortedFindings.length > 0 ? (
            <ul class="space-y-3 mb-4">
              {sortedFindings.map((f) => {
                const mine = f.author === myAgent.value;
                return (
                  <li
                    key={f.hash}
                    class="border border-gray-800 rounded-lg p-3 bg-gray-950/40"
                  >
                    <div class="flex items-baseline gap-2 mb-1.5">
                      <span
                        class={[
                          "text-xs font-medium",
                          mine ? "text-indigo-300" : "text-gray-400",
                        ].join(" ")}
                      >
                        {mine ? "You" : shortAgent(f.author)}
                      </span>
                      <span class="text-xs text-gray-600">
                        {formatTime(f.created_at)}
                      </span>
                    </div>
                    <p class="text-sm text-gray-200 whitespace-pre-wrap leading-relaxed">
                      {f.text}
                    </p>
                  </li>
                );
              })}
            </ul>
          ) : (
            <p class="text-sm text-gray-500 mb-4">
              No findings yet. Record what you noticed while testing this
              scenario.
            </p>
          )}

          {linked.value ? (
            <div>
              <textarea
                value={findingInput.value}
                onInput$={(_, el) => (findingInput.value = el.value)}
                disabled={findingSubmitting.value}
                rows={3}
                placeholder="What did you notice? Steps, unexpected behaviour, anything worth recording…"
                class="w-full bg-gray-950 border border-gray-800 rounded-lg p-3 text-sm text-gray-200 placeholder-gray-600 focus:outline-none focus:border-gray-600 resize-y disabled:opacity-50"
              />
              <div class="flex items-center justify-between mt-2">
                {findingError.value ? (
                  <p class="text-xs text-red-400">{findingError.value}</p>
                ) : (
                  <span />
                )}
                <button
                  type="button"
                  disabled={
                    findingSubmitting.value || !findingInput.value.trim()
                  }
                  onClick$={submitFinding}
                  class="text-sm bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white px-4 py-2 rounded-lg font-medium transition-colors"
                >
                  {findingSubmitting.value ? "Adding…" : "Add finding"}
                </button>
              </div>
            </div>
          ) : (
            <p class="text-sm text-gray-500">
              Sign in with Flowsta to add a finding.
            </p>
          )}
        </div>
      </div>
    </div>
  );
});
