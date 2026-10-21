import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vite-plus/test";

import { copyText } from "./copy-text";

interface FakeTextarea {
  value: string;
  removed: boolean;
  selected: boolean;
  style: Record<string, string>;
  remove: () => void;
  select: () => void;
  setAttribute: () => void;
  setSelectionRange: () => void;
}

function fakeTextarea(): FakeTextarea {
  const textarea: FakeTextarea = {
    remove: () => {
      textarea.removed = true;
    },
    removed: false,
    select: () => {
      textarea.selected = true;
    },
    selected: false,
    setAttribute: () => {},
    setSelectionRange: () => {},
    style: {},
    value: "",
  };
  return textarea;
}

/** Stand-in for the bits of `document` the fallback touches. */
function stubDocument(execCommand: () => boolean) {
  const textarea = fakeTextarea();
  const body = { append: vi.fn<(node: unknown) => void>() };
  const dialog = { append: vi.fn<(node: unknown) => void>() };
  const focus = vi.fn<() => void>();
  const activeElement = {
    closest: vi.fn<() => unknown>(() => null),
    focus,
  };
  const exec = vi.fn<(command: string) => boolean>(execCommand);

  vi.stubGlobal("document", {
    activeElement,
    body,
    createElement: () => textarea,
    execCommand: exec,
  });
  return { activeElement, body, dialog, exec, focus, textarea };
}

describe("copyText", () => {
  beforeEach(() => {
    vi.stubGlobal("navigator", {});
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uses the Clipboard API when it exists", async () => {
    const writeText = vi
      .fn<(text: string) => Promise<void>>()
      .mockResolvedValue();
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const { exec } = stubDocument(() => true);

    expect(await copyText("hello")).toBe(true);
    expect(writeText).toHaveBeenCalledWith("hello");
    expect(exec).not.toHaveBeenCalled();
  });

  // The reported bug: over plain http `navigator.clipboard` is undefined.
  it("falls back to execCommand when navigator.clipboard is missing", async () => {
    const { body, exec, textarea } = stubDocument(() => true);

    expect(await copyText("hello")).toBe(true);
    expect(textarea.value).toBe("hello");
    expect(textarea.selected).toBe(true);
    expect(body.append).toHaveBeenCalledWith(textarea);
    expect(exec).toHaveBeenCalledWith("copy");
    expect(textarea.removed).toBe(true);
  });

  it("falls back when writeText rejects (permission denied)", async () => {
    const writeText = vi
      .fn<(text: string) => Promise<void>>()
      .mockRejectedValue(new Error("denied"));
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const { exec } = stubDocument(() => true);

    expect(await copyText("hello")).toBe(true);
    expect(exec).toHaveBeenCalledWith("copy");
  });

  it("returns false when execCommand reports failure", async () => {
    const { textarea } = stubDocument(() => false);

    expect(await copyText("hello")).toBe(false);
    expect(textarea.removed).toBe(true);
  });

  it("returns false when execCommand throws, and still cleans up", async () => {
    const { textarea } = stubDocument(() => {
      throw new Error("unsupported");
    });

    expect(await copyText("hello")).toBe(false);
    expect(textarea.removed).toBe(true);
  });

  it("mounts inside the open dialog so a focus trap keeps the selection", async () => {
    const { activeElement, body, dialog, textarea } = stubDocument(() => true);
    activeElement.closest.mockReturnValue(dialog);

    await copyText("hello");
    expect(dialog.append).toHaveBeenCalledWith(textarea);
    expect(body.append).not.toHaveBeenCalled();
  });

  it("gives focus back to the element that had it", async () => {
    const { focus } = stubDocument(() => true);

    await copyText("hello");
    expect(focus).toHaveBeenCalled();
  });
});
