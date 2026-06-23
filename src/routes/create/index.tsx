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
import { createItem, getAllItems } from "~/lib/holochain";

export default component$(() => {
  const linked = useContext(linkedContext);
  const nav = useNavigate();

  // Campaign + section stay sticky across submits so several scenarios can be
  // seeded into the same group without retyping. Title/what-to-do/look-for
  // clear after each add.
  const campaign = useSignal("R&O v0.4.0");
  const section = useSignal("");
  const title = useSignal("");
  const instructions = useSignal("");
  const lookFor = useSignal("");

  const nextOrder = useSignal(1);
  const submitting = useSignal(false);
  const addedCount = useSignal(0);
  const justAdded = useSignal<string | null>(null);
  const error = useSignal<string | null>(null);

  // Seed the order counter from existing scenarios (max order + 1) so new
  // ones append in sequence. Re-synced whenever this route mounts.
  useVisibleTask$(async () => {
    try {
      const items = await getAllItems();
      const maxOrder = items.reduce((m, it) => Math.max(m, it.item.order), 0);
      nextOrder.value = maxOrder + 1;
    } catch {
      nextOrder.value = 1;
    }
  });

  const submit = $(async () => {
    error.value = null;

    const c = campaign.value.trim();
    const s = section.value.trim();
    const t = title.value.trim();
    if (!t) {
      error.value = "Title is required";
      return;
    }
    if (!s) {
      error.value = "Section is required";
      return;
    }

    submitting.value = true;
    try {
      await createItem({
        kind: "Scenario",
        campaign: c,
        section: s,
        title: t,
        instructions: instructions.value.trim(),
        look_for: lookFor.value.trim(),
        order: nextOrder.value,
      });

      // Stay on the form: record the add, bump the order, clear the
      // per-scenario fields, keep campaign + section for the next one.
      justAdded.value = t;
      addedCount.value += 1;
      nextOrder.value += 1;
      title.value = "";
      instructions.value = "";
      lookFor.value = "";
    } catch (e: any) {
      error.value = e?.message || String(e) || "Failed to create scenario";
    } finally {
      submitting.value = false;
    }
  });

  if (!linked.value) {
    return (
      <div class="max-w-xl mx-auto text-center py-16">
        <h1 class="text-2xl font-bold mb-4">Add scenario</h1>
        <p class="text-gray-400 mb-6">
          Sign in with Flowsta to seed scenarios.
        </p>
        <button
          type="button"
          onClick$={() => {
            setSignInIntent({ autoLink: true, returnTo: "/create/" });
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

  return (
    <div class="max-w-xl mx-auto">
      <h1 class="text-2xl font-bold mb-6">Add scenario</h1>

      {error.value && (
        <div class="bg-red-900/50 border border-red-700 text-red-300 px-4 py-3 rounded-lg mb-4">
          {error.value}
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
            onInput$={(e) => (campaign.value = (e.target as HTMLInputElement).value)}
            class="w-full bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-indigo-500"
            placeholder="e.g. R&O v0.4.0"
          />
        </div>

        <div>
          <label class="block text-sm font-medium text-gray-300 mb-1">
            Section
          </label>
          <input
            type="text"
            value={section.value}
            onInput$={(e) => (section.value = (e.target as HTMLInputElement).value)}
            class="w-full bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-indigo-500"
            placeholder="e.g. Installation & First Launch"
          />
        </div>

        <div>
          <label class="block text-sm font-medium text-gray-300 mb-1">
            Title
          </label>
          <input
            type="text"
            value={title.value}
            onInput$={(e) => (title.value = (e.target as HTMLInputElement).value)}
            class="w-full bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-indigo-500"
            placeholder="What the tester should attempt"
          />
        </div>

        <div>
          <label class="block text-sm font-medium text-gray-300 mb-1">
            What to do <span class="text-gray-500 font-normal">(optional)</span>
          </label>
          <textarea
            value={instructions.value}
            onInput$={(e) =>
              (instructions.value = (e.target as HTMLTextAreaElement).value)
            }
            class="w-full bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-indigo-500 h-32 resize-none"
            placeholder="Steps to follow…"
          />
        </div>

        <div>
          <label class="block text-sm font-medium text-gray-300 mb-1">
            Look for <span class="text-gray-500 font-normal">(optional)</span>
          </label>
          <textarea
            value={lookFor.value}
            onInput$={(e) =>
              (lookFor.value = (e.target as HTMLTextAreaElement).value)
            }
            class="w-full bg-gray-900 border border-gray-700 rounded-lg px-3 py-2 text-white focus:outline-none focus:border-indigo-500 h-24 resize-none"
            placeholder="Expected outcome / what success looks like…"
          />
        </div>

        {justAdded.value && (
          <div class="bg-green-900/20 border border-green-800 text-green-300 px-4 py-2 rounded-lg text-sm flex items-center justify-between gap-3">
            <span>
              Added “{justAdded.value}” · {addedCount.value} this session
            </span>
            <Link href="/" class="underline shrink-0">
              View board
            </Link>
          </div>
        )}

        <button
          type="button"
          onClick$={submit}
          disabled={submitting.value}
          class="w-full bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white font-medium py-2.5 rounded-full"
        >
          {submitting.value ? "Adding…" : `Add scenario #${nextOrder.value}`}
        </button>
      </div>
    </div>
  );
});
