import {
  component$,
  useContext,
  useSignal,
  useVisibleTask$,
  $,
} from "@builder.io/qwik";
import { Link } from "@builder.io/qwik-city";

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
  loadMyAgentSet,
  type Item,
  type ResponseData,
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

      responses.value = await getItemResponses(hash);
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
      </div>

      <div class="space-y-5">
        {it.instructions.trim() && (
          <div class="bg-gray-900 border border-gray-800 rounded-lg p-5">
            <h2 class="text-sm font-medium text-gray-300 mb-2">What to do</h2>
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

        {!it.instructions.trim() && !it.look_for.trim() && (
          <p class="text-gray-500 text-sm">
            No instructions recorded for this scenario yet.
          </p>
        )}

        {/* ── Verdict control ── */}
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
      </div>
    </div>
  );
});
