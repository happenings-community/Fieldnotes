import {
  component$,
  useSignal,
  useComputed$,
  useVisibleTask$,
  $,
} from "@builder.io/qwik";
import { useNavigate } from "@builder.io/qwik-city";
import { getAllItems, type ItemListItem } from "~/lib/holochain";
import { formatInvokeError } from "~/lib/errors";

export default component$(() => {
  const nav = useNavigate();
  const items = useSignal<ItemListItem[]>([]);
  const loading = useSignal(true);
  const loadingSlow = useSignal(false);
  const error = useSignal<string | null>(null);

  const loadItems = $(async () => {
    loading.value = true;
    error.value = null;
    try {
      items.value = await getAllItems();
    } catch (e: any) {
      error.value = formatInvokeError(e, "Failed to load scenarios");
    } finally {
      loading.value = false;
    }
  });

  // Group by section. Sections are ordered by their lowest-order scenario,
  // and scenarios within a section by `order`, so the board follows the
  // campaign's own sequence rather than DHT link order.
  const grouped = useComputed$(() => {
    const bySection = new Map<string, ItemListItem[]>();
    for (const it of items.value) {
      const s = it.item.section?.trim() || "Uncategorised";
      const arr = bySection.get(s);
      if (arr) arr.push(it);
      else bySection.set(s, [it]);
    }
    for (const arr of bySection.values()) {
      arr.sort((a, b) => a.item.order - b.item.order);
    }
    return Array.from(bySection.entries()).sort(
      ([, a], [, b]) => (a[0]?.item.order ?? 0) - (b[0]?.item.order ?? 0),
    );
  });

  useVisibleTask$(({ cleanup }) => {
    const timer = setTimeout(() => {
      loadingSlow.value = true;
    }, 3000);
    cleanup(() => clearTimeout(timer));

    loadItems();

    // Silently re-fetch every 30s so peer-seeded scenarios appear without a
    // manual reload. Update items directly so the list doesn't flash a
    // loading state on each tick.
    const refresh = setInterval(async () => {
      try {
        items.value = await getAllItems();
      } catch {
        // Transient failure — the next tick retries.
      }
    }, 30_000);
    cleanup(() => clearInterval(refresh));
  });

  return (
    <div>
      <div class="flex items-center justify-between mb-6">
        <h1 class="text-2xl font-bold">Scenarios</h1>
        {!loading.value && !error.value && items.value.length > 0 && (
          <span class="text-sm text-gray-500">
            {items.value.length} scenario{items.value.length !== 1 ? "s" : ""}
          </span>
        )}
      </div>

      {loading.value ? (
        <div class="text-gray-400">
          <p>Loading scenarios...</p>
          {loadingSlow.value && (
            <p class="text-gray-500 text-sm mt-2">
              Syncing with the network — first load can take a moment.
            </p>
          )}
        </div>
      ) : error.value ? (
        <div class="bg-red-900/20 border border-red-800/40 rounded-lg p-5 max-w-md">
          <p class="text-red-300 text-sm font-medium mb-1">
            Couldn't load scenarios
          </p>
          <p class="text-red-400/70 text-xs mb-3">{error.value}</p>
          <button
            type="button"
            onClick$={() => loadItems()}
            class="text-xs bg-red-800/40 hover:bg-red-800/60 text-red-300 px-3 py-1.5 rounded-full font-medium transition-colors"
          >
            Try again
          </button>
        </div>
      ) : items.value.length === 0 ? (
        <div class="text-center py-16">
          <p class="text-gray-400 text-lg mb-2">No scenarios yet</p>
          <p class="text-gray-500 text-sm max-w-md mx-auto">
            Scenarios are seeded by the campaign owner. Once they're imported
            they'll appear here, grouped by section. If a peer has just seeded
            them, they'll sync in within a minute or two.
          </p>
        </div>
      ) : (
        <div class="space-y-8">
          {grouped.value.map(([section, scenarios]) => (
            <section key={section}>
              <h2 class="text-xs font-semibold uppercase tracking-wider text-gray-500 mb-3">
                {section}
              </h2>
              <ul class="border border-gray-800 rounded-lg overflow-hidden divide-y divide-gray-800">
                {scenarios.map((s) => (
                  <li
                    key={s.hash}
                    onClick$={() => nav(`/poll/#${s.hash}`)}
                    class="px-4 py-3 bg-gray-900/40 hover:bg-gray-900 transition-colors flex items-baseline gap-3 cursor-pointer"
                  >
                    <span class="text-gray-600 text-xs tabular-nums w-6 shrink-0 text-right">
                      {s.item.order}
                    </span>
                    <span class="text-sm text-gray-200">{s.item.title}</span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
    </div>
  );
});
