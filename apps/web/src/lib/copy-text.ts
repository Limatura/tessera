/**
 * `navigator.clipboard` only exists in secure contexts, so over plain http
 * (a LAN address, a self-hosted box without TLS) it is `undefined` and calling
 * it throws. Fall back to selecting a hidden textarea and `execCommand("copy")`.
 *
 * Resolves `false` when neither path worked, so callers only show "copied" on
 * a real copy.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Insecure context or permission denied: try the legacy path.
  }
  return legacyCopy(text);
}

function legacyCopy(text: string): boolean {
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "");
  Object.assign(textarea.style, {
    left: "-9999px",
    opacity: "0",
    position: "fixed",
    top: "0",
  });

  // Append inside the open dialog (if any) so a focus trap doesn't steal focus
  // back and make the selection collapse.
  const host =
    document.activeElement?.closest("dialog, [role='dialog']") ?? document.body;
  const previousFocus = document.activeElement as HTMLElement | null;
  host.append(textarea);
  textarea.select();
  textarea.setSelectionRange(0, text.length);

  try {
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    textarea.remove();
    previousFocus?.focus?.();
  }
}
