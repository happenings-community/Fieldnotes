import {
  component$,
  useSignal,
  useVisibleTask$,
  $,
} from "@builder.io/qwik";
import { useNavigate, Link } from "@builder.io/qwik-city";
import { addAdministrator, getAdministrators, isAdministrator } from "~/lib/holochain";

export default component$(() => {
  const nav = useNavigate();
  const isAdmin = useSignal(false);
  const admins = useSignal<string[]>([]);
  const adminPubkey = useSignal("");
  const submitting = useSignal(false);
  const error = useSignal<string | null>(null);
  const message = useSignal<string | null>(null);

  // Check if current user is admin and load admin list
  useVisibleTask$(async () => {
    try {
      const admin = await isAdministrator();
      isAdmin.value = admin;
      const adminList = await getAdministrators();
      admins.value = adminList;
    } catch (e) {
      error.value = `Failed to load admin status: ${e}`;
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

      const grantHash = await addAdministrator(pk);
      message.value = `Administrator added. Grant: ${grantHash.slice(0, 16)}...`;
      adminPubkey.value = "";

      // Refresh the admin list
      const adminList = await getAdministrators();
      admins.value = adminList;
      submitting.value = false;
    } catch (e) {
      error.value = `Error: ${e}`;
      submitting.value = false;
    }
  });

  return (
    <div class="min-h-screen bg-gray-900 text-gray-100">
      <div class="max-w-2xl mx-auto p-6">
        <Link href="/" class="text-indigo-400 hover:text-indigo-300 mb-6 inline-block">
          ← Back
        </Link>

        <h1 class="text-3xl font-bold mb-8">Admin Dashboard</h1>

        <div class={`p-3 rounded mb-8 text-sm ${isAdmin.value ? "bg-green-900 border border-green-700" : "bg-gray-800 border border-gray-700"}`}>
          {isAdmin.value
            ? "You are an administrator."
            : "You are not currently an administrator. Only the progenitor can issue valid grants (signed with the progenitor key)."}
        </div>

        <section class="mb-12">
          <h2 class="text-2xl font-bold mb-4">Current Administrators</h2>
          {admins.value.length === 0 ? (
            <p class="text-gray-400">No administrators yet</p>
          ) : (
            <ul class="space-y-2">
              {admins.value.map((admin) => (
                <li key={admin} class="bg-gray-800 p-3 rounded font-mono text-sm break-all">
                  {admin}
                </li>
              ))}
            </ul>
          )}
        </section>

        <section>
          <h2 class="text-2xl font-bold mb-4">Add Administrator</h2>
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
          <div class="space-y-4">
            <input
              type="text"
              placeholder="Admin Flowsta agent pubkey (uhCAk...)"
              value={adminPubkey.value}
              onInput$={(_, el) => (adminPubkey.value = el.value)}
              class="w-full bg-gray-800 border border-gray-700 p-3 rounded text-gray-100 placeholder-gray-500 font-mono"
            />
            <button
              onClick$={handleAddAdmin}
              disabled={submitting.value}
              class="bg-indigo-600 hover:bg-indigo-700 disabled:bg-gray-600 text-white font-bold py-2 px-4 rounded"
            >
              {submitting.value ? "Adding..." : "Add Administrator"}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
});
