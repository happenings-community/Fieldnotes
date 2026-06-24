import {
  component$,
  useContext,
  useSignal,
  useVisibleTask$,
  $,
} from "@builder.io/qwik";
import { useNavigate, Link } from "@builder.io/qwik-city";
import { linkedContext } from "~/lib/context";
import { setSignInIntent } from "~/lib/signin";
import { importItems, getAllItems, getAdminGrantHash, type CreateItemInput } from "~/lib/holochain";

// Canonical target for R&O v0.4.0 (see the test-tracker templates.json).
const EXPECTED_ITEMS = 57;
const EXPECTED_SECTIONS = 12;

// Identity for de-duplication: a scenario is "the same" if campaign + title
// match. Title carries the stepId prefix, so it's unique within a campaign.
// NUL-join so the two fields can't collide across the boundary.
const itemKey = (campaign: string, title: string) => campaign + "\u0000" + title;

/**
 * Robust error → string. Tauri/Holochain often reject with a BARE STRING,
 * which `e?.message` alone silently swallows (the board bug). Surfaces the
 * real serde error verbatim if anything in the host path ever complains.
 */
function errText(e: any): string {
  if (e == null) return "Unknown error";
  if (typeof e === "string") return e;
  if (typeof e.message === "string") return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

/**
 * Map a templates.json array → CreateItemInput[].
 *   title        = "<stepId> — <first |-segment>"   (stepId prepended for traceability)
 *   instructions = remaining |-segments, newline-joined
 *   look_for     = lookFor |-split, newline-joined
 *   section      = testArea (verbatim)
 *   campaign     = supplied (constant for a run)
 *   kind         = "Scenario"
 *   order        = 1-based array index (NOT derived from stepId)
 * Throws with a readable message on malformed input.
 */
function mapTemplates(text: string, campaign: string): CreateItemInput[] {
  const data = JSON.parse(text);
  if (!Array.isArray(data)) {
    throw new Error("Expected a JSON array of step objects.");
  }
  const items: CreateItemInput[] = [];
  const bad: number[] = [];

  data.forEach((o: any, i: number) => {
    const idOk =
      o && (typeof o.stepId === "number" || typeof o.stepId === "string");
    const ok =
      idOk &&
      typeof o.testArea === "string" &&
      typeof o.stepAction === "string" &&
      typeof o.lookFor === "string";
    if (!ok) {
      bad.push(i);
      return;
    }

    const parts = String(o.stepAction)
      .split("|")
      .map((s: string) => s.trim());
    const title = `${String(o.stepId)} — ${parts[0]}`;
    const instructions = parts
      .slice(1)
      .filter((s: string) => s.length > 0)
      .join("\n");
    const look_for = String(o.lookFor)
      .split("|")
      .map((s: string) => s.trim())
      .filter((s: string) => s.length > 0)
      .join("\n");

    items.push({
      kind: "Scenario",
      campaign,
      section: o.testArea,
      title,
      instructions,
      look_for,
      order: i + 1,
    });
  });

  if (bad.length > 0) {
    throw new Error(
      `${bad.length} of ${data.length} entries are missing required fields ` +
        `(stepId, testArea, stepAction, lookFor). First problem at index ${bad[0]}.`,
    );
  }
  return items;
}

export default component$(() => {
  const linked = useContext(linkedContext);
  const nav = useNavigate();

  const raw = useSignal("");
  const campaign = useSignal("R&O v0.4.0");
  const parsed = useSignal<CreateItemInput[] | null>(null);
  const parseError = useSignal<string | null>(null);

  const importing = useSignal(false);
  const importError = useSignal<string | null>(null);
  // null = test not run; otherwise how many NEW the test added.
  const testResult = useSignal<number | null>(null);

  // Snapshot of what's already on the board, for de-duplication.
  const existingKeys = useSignal<string[]>([]);
  const boardCount = useSignal(0);

  // Load the current board on mount so the preview can show new-vs-existing.
  useVisibleTask$(async () => {
    try {
      const ex = await getAllItems();
      existingKeys.value = ex.map((e) => itemKey(e.item.campaign, e.item.title));
      boardCount.value = ex.length;
    } catch {
      existingKeys.value = [];
      boardCount.value = 0;
    }
  });

  const doPreview = $(() => {
    parseError.value = null;
    parsed.value = null;
    testResult.value = null;
    const text = raw.value.trim();
    if (!text) {
      parseError.value = "Paste the templates.json array, or choose a file.";
      return;
    }
    try {
      parsed.value = mapTemplates(text, campaign.value.trim() || "R&O v0.4.0");
    } catch (e: any) {
      parseError.value =
        e?.name === "SyntaxError"
          ? `Couldn't parse JSON: ${e.message}`
          : errText(e);
    }
  });

  // count given  → test slice (stay on screen, report how many were new).
  // count omitted → full import (navigate to the board).
  // Either way: dedupe against the live board, create only what's missing.
  const runImport = $(async (count?: number) => {
    const all = parsed.value;
    if (!all || all.length === 0) return;
    const isTest = typeof count === "number";

    importing.value = true;
    importError.value = null;
    try {
      // Authoritative dedupe: re-read the board now, not the mount snapshot.
      const ex = await getAllItems();
      const have = new Set(
        ex.map((e) => itemKey(e.item.campaign, e.item.title)),
      );
      const candidates = isTest ? all.slice(0, count) : all;
      const toCreate = candidates.filter(
        (m) => !have.has(itemKey(m.campaign, m.title)),
      );

      if (toCreate.length > 0) {
        // Scenarios are admin-gated: every item must carry the importer's
        // AdminGrant hash or validate_item rejects it. Fetch once, stamp all.
        const grantHash = await getAdminGrantHash();
        if (!grantHash) {
          importError.value =
            "You need administrator authority to import scenarios. " +
            "Become an administrator on the Identity screen first.";
          importing.value = false;
          return;
        }
        const stamped = toCreate.map((m) => ({
          ...m,
          admin_grant_action_hash: grantHash,
        }));
        await importItems(stamped);
      }

      if (isTest) {
        // Refresh the snapshot so the preview counts reflect the test add.
        const ex2 = await getAllItems();
        existingKeys.value = ex2.map((e) =>
          itemKey(e.item.campaign, e.item.title),
        );
        boardCount.value = ex2.length;
        testResult.value = toCreate.length;
      } else {
        nav("/");
      }
    } catch (e: any) {
      importError.value = errText(e);
    } finally {
      importing.value = false;
    }
  });

  if (!linked.value) {
    return (
      <div class="max-w-xl mx-auto text-center py-16">
        <h1 class="text-2xl font-bold mb-4">Import scenarios</h1>
        <p class="text-gray-400 mb-6">Sign in with Flowsta to import scenarios.</p>
        <button
          type="button"
          onClick$={() => {
            setSignInIntent({ autoLink: true, returnTo: "/import/" });
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

  // Derived preview stats (re-run when parsed/existing change).
  const items = parsed.value;
  let sectionRows: { section: string; count: number }[] = [];
  if (items) {
    const orderSeen: string[] = [];
    const counts = new Map<string, number>();
    for (const it of items) {
      if (!counts.has(it.section)) orderSeen.push(it.section);
      counts.set(it.section, (counts.get(it.section) || 0) + 1);
    }
    sectionRows = orderSeen.map((s) => ({
      section: s,
      count: counts.get(s) || 0,
    }));
  }
  const total = items?.length || 0;
  const sectionCount = sectionRows.length;
  const matchesCanonical =
    total === EXPECTED_ITEMS && sectionCount === EXPECTED_SECTIONS;
  const sample = items && items.length > 0 ? items[0] : null;

  // New-vs-existing against the board snapshot.
  const have = new Set(existingKeys.value);
  const skipCount = items
    ? items.filter((m) => have.has(itemKey(m.campaign, m.title))).length
    : 0;
  const newCount = total - skipCount;

  return (
    <div class="max-w-xl mx-auto">
      <h1 class="text-2xl font-bold mb-2">Import scenarios</h1>
      <p class="text-gray-400 text-sm mb-6">
        Paste the <code class="text-gray-300">templates.json</code> array (or
        choose the file), preview, then import. Re-importing is safe — anything
        already on the board is skipped.
      </p>

      {parseError.value && (
        <div class="bg-red-900/50 border border-red-700 text-red-300 px-4 py-3 rounded-lg mb-4">
          {parseError.value}
        </div>
      )}
      {importError.value && (
        <div class="bg-red-900/50 border border-red-700 text-red-300 px-4 py-3 rounded-lg mb-4">
          <div class="font-medium mb-1">Import failed</div>
          <div class="text-sm break-words">{importError.value}</div>
        </div>
      )}

      <div class="space-y-5">
        <div>
          <label class="block text-sm font-medium text-gray-300 mb-1">
            Campaign
          </label>
          <input
            type="text"
            value={campaign.value}
            onInput$={(e) =>
              (campaign.value = (e.target as HTMLInputElement).value)
            }
            class="w-full bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-indigo-500"
            placeholder="e.g. R&O v0.4.0"
          />
        </div>

        <div>
          <label class="block text-sm font-medium text-gray-300 mb-1">
            templates.json
          </label>
          <textarea
            value={raw.value}
            onInput$={(e) =>
              (raw.value = (e.target as HTMLTextAreaElement).value)
            }
            class="w-full bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-white font-mono text-xs focus:outline-none focus:border-indigo-500 h-40 resize-none"
            placeholder='[ { "stepId": 1.1, "testArea": "…", "stepAction": "…|…", "lookFor": "…|…" }, … ]'
          />
          <div class="mt-2">
            <input
              type="file"
              accept="application/json,.json"
              onChange$={async (_, el) => {
                const file = el.files?.[0];
                if (!file) return;
                raw.value = await file.text();
                await doPreview();
              }}
              class="block w-full text-sm text-gray-400 file:mr-3 file:rounded-full file:border-0 file:bg-gray-800 file:px-3 file:py-1.5 file:text-gray-200 hover:file:bg-gray-700"
            />
          </div>
        </div>

        <button
          type="button"
          onClick$={doPreview}
          class="w-full bg-gray-800 hover:bg-gray-700 text-white font-medium py-2.5 rounded-full"
        >
          Preview
        </button>

        {items && (
          <div class="space-y-4 pt-2">
            <div
              class={
                "px-4 py-3 rounded-lg border " +
                (matchesCanonical
                  ? "bg-green-900/20 border-green-800 text-green-300"
                  : "bg-amber-900/20 border-amber-800 text-amber-300")
              }
            >
              <div class="font-medium">
                {total} scenario{total === 1 ? "" : "s"} across {sectionCount}{" "}
                section{sectionCount === 1 ? "" : "s"}
              </div>
              <div class="text-sm opacity-80">
                {matchesCanonical
                  ? "Matches the R&O v0.4.0 canonical set."
                  : `Expected ${EXPECTED_ITEMS} across ${EXPECTED_SECTIONS} for R&O v0.4.0 — double-check the source.`}
              </div>
            </div>

            <div class="text-sm text-gray-400 px-1">
              Board holds {boardCount.value}. This source:{" "}
              <span class="text-gray-200">{newCount} new</span>
              {skipCount > 0 && (
                <span>
                  , <span class="text-gray-500">{skipCount} already present</span>
                </span>
              )}
              .
            </div>

            <div class="border border-gray-800 rounded-lg divide-y divide-gray-800">
              {sectionRows.map((r, i) => (
                <div
                  key={r.section}
                  class="flex items-center justify-between px-4 py-2 text-sm"
                >
                  <span class="text-gray-300">
                    <span class="text-gray-500 mr-2">{i + 1}.</span>
                    {r.section}
                  </span>
                  <span class="text-gray-500">{r.count}</span>
                </div>
              ))}
            </div>

            {sample && (
              <div class="border border-gray-800 rounded-lg px-4 py-3 text-sm">
                <div class="text-gray-500 uppercase tracking-wide text-xs mb-2">
                  First item (mapping check)
                </div>
                <div class="text-white font-medium mb-1">{sample.title}</div>
                {sample.instructions && (
                  <div class="text-gray-400 whitespace-pre-line mb-1">
                    {sample.instructions}
                  </div>
                )}
                {sample.look_for && (
                  <div class="text-gray-500 whitespace-pre-line">
                    <span class="text-gray-600">Look for: </span>
                    {sample.look_for}
                  </div>
                )}
              </div>
            )}

            {testResult.value !== null && (
              <div
                class={
                  "px-4 py-3 rounded-lg text-sm flex items-center justify-between gap-3 border " +
                  (testResult.value > 0
                    ? "bg-green-900/20 border-green-800 text-green-300"
                    : "bg-amber-900/20 border-amber-800 text-amber-300")
                }
              >
                <span>
                  {testResult.value > 0
                    ? `Test import landed — ${testResult.value} new. Check the board, then import the rest below.`
                    : "Those were already on the board — nothing added."}
                </span>
                <Link href="/" class="underline shrink-0">
                  View board
                </Link>
              </div>
            )}

            <div class="space-y-2">
              <button
                type="button"
                onClick$={() => runImport()}
                disabled={importing.value || newCount === 0}
                class="w-full bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white font-medium py-2.5 rounded-full"
              >
                {importing.value
                  ? "Importing…"
                  : newCount === 0
                    ? "Nothing new to import"
                    : `Import ${newCount} new scenario${newCount === 1 ? "" : "s"}`}
              </button>
              <button
                type="button"
                onClick$={() => runImport(2)}
                disabled={importing.value || total < 2}
                class="w-full bg-transparent border border-gray-700 hover:border-gray-500 text-gray-300 text-sm py-2 rounded-full disabled:opacity-50"
              >
                Run a 2-item test first
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
});
