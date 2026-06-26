import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";

/**
 * Bring Fieldnotes's own window back to the foreground after a cross-app flow
 * (e.g. Flowsta sign-in, where the Vault popped up to ask for approval), so the
 * user lands back here and sees the result.
 *
 * Best-effort: Vault already yields focus back to us after the user responds,
 * so this mostly matters when Vault was already open and stayed in front — in
 * which case `requestUserAttention` flashes our taskbar entry. No-op if we're
 * already focused, and never throws.
 */
export async function focusSelf(): Promise<void> {
  try {
    const w = getCurrentWindow();
    await w.show();
    await w.unminimize();
    await w.setFocus();
    await w.requestUserAttention(UserAttentionType.Informational);
  } catch {
    // best-effort — focus is a nicety, not a requirement
  }
}
