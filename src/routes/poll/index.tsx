import { component$, useSignal, useVisibleTask$ } from "@builder.io/qwik";
import { Link } from "@builder.io/qwik-city";

// Why hash-based instead of /poll/[id]/?
// The Qwik static adapter only pre-renders routes whose param values are
// enumerated up-front. Scenario action hashes aren't, so navigating to
// /poll/<unknown-hash>/ fails: the per-route q-data.json doesn't exist, the
// client router silently aborts the render, and the click looks like a no-op.
// Using /poll/#<hash> sidesteps it — /poll/ is a fully static route, the hash
// is browser-only (never sent to the server-shaped asset resolver), and
// switching scenarios is just a hash change.
import { formatInvokeError } from "~/lib/errors";
import { getItem, type Item } from "~/lib/holochain";

export default component$(() => {
  // The scenario's action hash is read from window.location.hash at client
  // visible-task time (the hash is never present on the server side).
  const itemHash = useSignal<string>("");
  const item = useSignal<Item | null>(null);
  const author = useSignal<string | null>(null);
  const loading = useSignal(true);
  const error = useSignal<string | null>(null);

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
      const result = await getItem(hash);
      if (!result) {
        error.value = "Scenario not found";
        return;
      }
      item.value = result.item;
      author.value = result.author;
    } catch (e: any) {
      error.value = formatInvokeError(e, "Failed to load scenario");
    } finally {
      loading.value = false;
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
      </div>
    </div>
  );
});
