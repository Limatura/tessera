export type PlayerAction =
  | { type: "toggle-play" }
  | { type: "seek-by"; seconds: number }
  /** A fraction of the duration: Home is `0`, End `1`, the digit 4 is `0.4`. */
  | { type: "seek-to"; fraction: number }
  | { type: "volume-by"; delta: number }
  | { type: "toggle-mute" }
  | { type: "toggle-fullscreen" };

/** Where the key was pressed, for keys a control answers on its own. */
export interface KeyContext {
  onButton: boolean;
  onSlider: boolean;
}

export const ARROW_SEEK_SECONDS = 5;
export const JUMP_SEEK_SECONDS = 10;
export const VOLUME_STEP = 0.1;

interface Shortcut {
  action: PlayerAction;
  /**
   * A focused button already answers Space by being clicked, and a slider
   * answers the arrows, Home and End by moving.
   */
  yieldTo?: keyof KeyContext;
}

const DIGITS = 10;

/** The keys YouTube taught everyone, letters in lower case. */
const SHORTCUTS: Record<string, Shortcut> = {
  " ": { action: { type: "toggle-play" }, yieldTo: "onButton" },
  ArrowDown: {
    action: { delta: -VOLUME_STEP, type: "volume-by" },
    yieldTo: "onSlider",
  },
  ArrowLeft: {
    action: { seconds: -ARROW_SEEK_SECONDS, type: "seek-by" },
    yieldTo: "onSlider",
  },
  ArrowRight: {
    action: { seconds: ARROW_SEEK_SECONDS, type: "seek-by" },
    yieldTo: "onSlider",
  },
  ArrowUp: {
    action: { delta: VOLUME_STEP, type: "volume-by" },
    yieldTo: "onSlider",
  },
  End: { action: { fraction: 1, type: "seek-to" }, yieldTo: "onSlider" },
  Home: { action: { fraction: 0, type: "seek-to" }, yieldTo: "onSlider" },
  f: { action: { type: "toggle-fullscreen" } },
  j: { action: { seconds: -JUMP_SEEK_SECONDS, type: "seek-by" } },
  k: { action: { type: "toggle-play" } },
  l: { action: { seconds: JUMP_SEEK_SECONDS, type: "seek-by" } },
  m: { action: { type: "toggle-mute" } },
  ...Object.fromEntries(
    Array.from({ length: DIGITS }, (_, digit) => [
      String(digit),
      { action: { fraction: digit / DIGITS, type: "seek-to" } },
    ])
  ),
};

/** What a key does in the player, or `null` when the player leaves it alone. */
export function shortcutFor(
  key: string,
  context: KeyContext
): PlayerAction | null {
  const shortcut = SHORTCUTS[key.length === 1 ? key.toLowerCase() : key];
  if (!shortcut || (shortcut.yieldTo && context[shortcut.yieldTo])) {
    return null;
  }
  return shortcut.action;
}
