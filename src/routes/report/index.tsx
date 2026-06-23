import {
  component$,
  useContext,
  useSignal,
  useComputed$,
  useVisibleTask$,
  $,
} from "@builder.io/qwik";
import { useNavigate, Link } from "@builder.io/qwik-city";
import { linkedContext } from "~/lib/context";
import { setSignInIntent } from "~/lib/signin";
import { formatInvokeError } from "~/lib/errors";
import {
  createItem,
  getAllItems,
  type ItemListItem,
} from "~/lib/holochain";

// Emergent issues are filed as Feedback items in their own section, so the
// board (which groups by section, kind-agnostically) gives them their own
// group automatically, and the detail screen gives each one a findings
// thread for free. No zome change — `kind: "Feedback"` already round-trips.
const FEEDBACK_SECTION = "Emergent issues";
const FALLBACK_CAMPAIGN = "R&O v0.4.0";

// ── Duplicate-surfacing (keyword overlap, no new infrastructure) ──────
//
// As the reporter types a summary, we float the existing issues most likely
// to be the same one to the top, so they catch a duplicate before filing.
// This is word-overlap scoring weighted by word rarity (a distinctive word
// like "crash" counts more than filler like "app") — NOT semantic matching.
// It catches the common case (people describing the same bug reach for some
// of the same words); it will miss dupes with no shared words ("won't open"
// vs "white screen"). Semantic/embedding matching is a later step to earn if
// this proves insufficient, and would stay local to keep the offline posture.
//
// The list NEVER hides issues destructively: with no summary, or when nothing
// scores above threshold, the full list shows exactly as before. Ranking is a
// layer on top, never a gate.

const STOPWORDS = new Set([
  "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "on",
  "in", "at", "to", "of", "for", "and", "or", "but", "with", "my", "it", "its",
  "this", "that", "when", "then", "there", "here", "not", "no", "you", "your",
  "app", "get", "got", "after", "before", "from", "into", "out", "up", "down",
  "just", "very", "so", "if", "i",
]);

function tokenize(text: string): string[] {
  return (text.toLowerCase().match(/[a-z0-9]+/g) || []).filter(
    (w) => w.length >= 3 && !STOPWORDS.has(w),
  );
}

// How many issues contain each word, for rarity weighting.
function buildDocFreq(issues: ItemListItem[]): Map<string, number> {
  const df = new Map<string, number>();
  for (const iss of issues) {
    const seen = new Set(
      tokenize(iss.item.title + " " + iss.item.instructions),
    );
    for (const w of seen) df.set(w, (df.get(w) || 0) + 1);
  }
  return df;
}

function scoreIssue(
  queryTokens: Set<string>,
  issue: ItemListItem,
  df: Map<string, number>,
  totalIssues: number,
): number {
  const issueTokens = new Set(
    tokenize(issue.item.title + " " + issue.item.instructions),
  );
  let score = 0;
  for (const w of queryTokens) {
    if (issueTokens.has(w)) {
      const freq = df.get(w) || 1;
      // IDF-ish: rarer word -> higher weight, so filler can't dominate.
      score += Math.log((totalIssues + 1) / freq);
    }
  }
  return score;
}

// Only surface matches at least this strong, and at most this many, so a
// novel bug isn't shown weak coincidental matches. NOTE: the score scales with
// corpus size (log((total+1)/freq)), so this fixed value is an alpha-scale
// calibration — at 1.0 a single distinctive shared word (~1.39 in a small
// corpus) surfaces a match while zero-overlap (0) stays filtered. Normalising
// the score so the threshold is corpus-size-independent is future work.
const MATCH_THRESHOLD = 1.0;
const MAX_MATCHES = 3;
const DEBOUNCE_MS = 250;

export default component$(() => {
  const linked = useContext(linkedContext);
  const nav = useNavigate();

  // All items, loaded once on mount. Used for the dedup list, the ranking
  // corpus, and deriving the active campaign + next order value.
  const items = useSignal<ItemListItem[]>([]);
  const loadingExisting = useSignal(true);

  // The summary doubles as a search box (it drives the ranked dedup list) and
  // as the issue's title when nothing matches and the reporter files new.
  // There is no separate "what happened" field on this screen: detail is
  // written as the first FINDING on the issue's detail page, so an issue's
  // body lives in exactly one place (the findings thread), never split between
  // `instructions` and a finding depending on which screen it was typed on.
  const summary = useSignal("");
  // Debounced mirror of `summary` — ranking keys off this so the list settles
  // a beat after the reporter stops typing rather than reshuffling per keystroke.
  const rankQuery = useSignal("");
  const submitting = useSignal(false);
  const error = useSignal<string | null>(null);

  // "Show all" override: once a reporter expands the full list under a set of
  // ranked matches, keep it expanded.
  const showAll = useSignal(false);

  useVisibleTask$(async () => {
    try {
      items.value = await getAllItems();
    } catch {
      // Non-fatal: the list just shows empty and the tester can still file.
      items.value = [];
    } finally {
      loadingExisting.value = false;
    }
  });

  // Debounce: copy `summary` into `rankQuery` 250ms after the last keystroke.
  useVisibleTask$(({ track, cleanup }) => {
    const s = track(() => summary.value);
    const t = setTimeout(() => {
      rankQuery.value = s;
      showAll.value = false; // a new query resets the expand state
    }, DEBOUNCE_MS);
    cleanup(() => clearTimeout(t));
  });

  // All open emergent issues, newest first — the fallback list and the corpus.
  const openIssues = useComputed$(() =>
    items.value
      .filter((it) => it.item.kind === "Feedback")
      .sort((a, b) => b.item.order - a.item.order),
  );

  // Ranked matches for the current (debounced) query. Empty when there's no
  // query or nothing clears the threshold — callers then show the full list.
  const rankedMatches = useComputed$(() => {
    const q = rankQuery.value.trim();
    const issues = openIssues.value;
    if (!q || issues.length === 0) return [] as ItemListItem[];

    const df = buildDocFreq(issues);
    const total = issues.length;
    const qTokens = new Set(tokenize(q));
    if (qTokens.size === 0) return [] as ItemListItem[];

    return issues
      .map((iss) => ({ iss, score: scoreIssue(qTokens, iss, df, total) }))
      .filter((r) => r.score >= MATCH_THRESHOLD)
      .sort((a, b) => b.score - a.score)
      .slice(0, MAX_MATCHES)
      .map((r) => r.iss);
  });

  // File a NEW issue: the typed line becomes the title; `instructions` is left
  // empty (the body goes in as the first finding on the detail page). Then land
  // on the new issue's detail screen so the reporter writes what happened there.
  const fileNew = $(async () => {
    error.value = null;

    const s = summary.value.trim();
    if (!s) {
      error.value = "Type what went wrong first";
      return;
    }

    const campaign =
      items.value.find((it) => it.item.campaign?.trim())?.item.campaign?.trim() ||
      FALLBACK_CAMPAIGN;

    const maxOrder = items.value.reduce(
      (m, it) => Math.max(m, it.item.order),
      0,
    );

    submitting.value = true;
    try {
      const hash = await createItem({
        kind: "Feedback",
        campaign,
        section: FEEDBACK_SECTION,
        title: s,
        instructions: "",
        look_for: "",
        order: maxOrder + 1,
      });
      nav(`/poll/#${hash}`);
    } catch (e: any) {
      error.value = formatInvokeError(e, "Failed to file the issue");
      submitting.value = false;
    }
  });

  if (!linked.value) {
    return (
      <div class="max-w-xl mx-auto text-center py-16">
        <h1 class="text-2xl font-bold mb-4">Report an issue</h1>
        <p class="text-gray-400 mb-6">
          Sign in with Flowsta to report an issue you've hit while testing.
        </p>
        <button
          type="button"
          onClick$={() => {
            setSignInIntent({ autoLink: true, returnTo: "/report/" });
            nav("/identity/");
          }}
          class="bg-transparent border-0 p-0 cursor-pointer"
        >
          <img
            src="/assets/flowsta-signin.svg"
            alt="Sign in with Flowsta"
            width={158}
            height={36}
            class="hover:opacity-80 transition-opacity mx-auto"
          />
        </button>
      </div>
    );
  }

  const typed = summary.value.trim().length > 0;
  // Mid-debounce: the live input has moved on from the ranked query, so a
  // result is still settling. Derived, not a separate flag.
  const searching =
    typed && summary.value.trim() !== rankQuery.value.trim();
  const hasMatches = rankedMatches.value.length > 0;
  // The list to render: ranked matches when we have them (unless the reporter
  // expanded everything), else the full list.
  const listToShow =
    hasMatches && !showAll.value ? rankedMatches.value : openIssues.value;

  return (
    <div class="max-w-xl mx-auto">
      <h1 class="text-2xl font-bold mb-2">Report an issue</h1>
      <p class="text-sm text-gray-400 mb-6">
        Hit something the scripted scenarios don't cover — a bug, a surprise,
        anything broken with no step for it? Describe it in a line first: we'll
        surface anything similar that's already been reported, so you can add to
        it instead of filing a duplicate. If nothing matches, you'll file a new
        one and land on its page to write up what happened.
      </p>

      {error.value && (
        <div class="bg-red-900/50 border border-red-700 text-red-300 px-4 py-3 rounded-lg mb-4">
          {error.value}
        </div>
      )}

      <div class="space-y-5">
        {/* The summary is the search box AND the eventual issue title. */}
        <div>
          <label class="block text-sm font-medium text-gray-300 mb-1">
            What went wrong?
          </label>
          <input
            type="text"
            value={summary.value}
            onInput$={(e) =>
              (summary.value = (e.target as HTMLInputElement).value)
            }
            class="w-full bg-gray-950 border border-gray-700 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-indigo-500"
            placeholder="One line: what's broken or surprising"
          />
        </div>

        {/* Search status: makes the dedup search visible, so a clean
            result reads as "we checked" rather than silence. */}
        {typed && (
          <p class="text-xs -mt-2">
            {searching ? (
              <span class="text-gray-500">Searching reported issues…</span>
            ) : hasMatches ? (
              <span class="text-amber-400/80">
                {rankedMatches.value.length} possible match
                {rankedMatches.value.length !== 1 ? "es" : ""} below — is
                yours one of them?
              </span>
            ) : openIssues.value.length > 0 ? (
              <span class="text-emerald-400/80">
                Nothing similar found — file it as new below.
              </span>
            ) : null}
          </p>
        )}

        {/* ── Already-reported list (re-ranks live as the line is typed) ── */}
        <div class="bg-gray-900 border border-gray-800 rounded-lg p-5">
          <div class="flex items-baseline justify-between mb-3">
            <h2 class="text-sm font-medium text-gray-300">
              {hasMatches && !showAll.value
                ? "Possibly the same"
                : "Already reported"}
            </h2>
            {!loadingExisting.value && !hasMatches && (
              <span class="text-xs text-gray-500">
                {openIssues.value.length} issue
                {openIssues.value.length !== 1 ? "s" : ""}
              </span>
            )}
          </div>

          {loadingExisting.value ? (
            <p class="text-sm text-gray-500">Loading reported issues…</p>
          ) : openIssues.value.length > 0 ? (
            <>
              <p class="text-xs text-gray-500 mb-3">
                {hasMatches && !showAll.value
                  ? "These look similar to what you're describing. Is yours here? Open it and add what you saw, rather than filing a duplicate."
                  : "Is yours one of these? Open it and add what you saw (your steps, your setup) rather than filing a duplicate."}
              </p>
              <ul class="border border-gray-800 rounded-lg overflow-hidden divide-y divide-gray-800 mb-3">
                {listToShow.map((iss) => (
                  <li
                    key={iss.hash}
                    onClick$={() => nav(`/poll/#${iss.hash}`)}
                    class="px-4 py-3 bg-gray-950/40 hover:bg-gray-900 transition-colors cursor-pointer"
                  >
                    <span class="text-sm text-gray-200">{iss.item.title}</span>
                  </li>
                ))}
              </ul>
              {hasMatches && !showAll.value && (
                <button
                  type="button"
                  onClick$={() => (showAll.value = true)}
                  class="text-xs text-indigo-400 hover:text-indigo-300 underline mb-1"
                >
                  None of these — show all {openIssues.value.length} reported
                  issue{openIssues.value.length !== 1 ? "s" : ""}
                </button>
              )}
            </>
          ) : (
            <p class="text-sm text-gray-500">
              Nothing reported yet. If you've hit something, you'll be the first
              to log it.
            </p>
          )}
        </div>

        {/* File-new: appears once a line is typed. Framed as "the search
            didn't find it" — the only way to file is to say nothing matched. */}
        {typed && (
          <div>
            <button
              type="button"
              onClick$={fileNew}
              disabled={submitting.value}
              class="w-full bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white font-medium py-2.5 rounded-full"
            >
              {submitting.value
                ? "Filing…"
                : "None of these — report it as new"}
            </button>
            <p class="text-xs text-gray-500 text-center mt-2">
              Files “{summary.value.trim()}” as a new issue and takes you to its
              page, where you and others add findings and the team follows up.
            </p>
          </div>
        )}
      </div>

      <p class="mt-6 text-sm text-gray-500">
        <Link href="/" class="text-indigo-400 hover:text-indigo-300 underline">
          ← Back to scenarios
        </Link>
      </p>
    </div>
  );
});
