import { describe, expect, it } from "vite-plus/test";

import { shortcutFor } from "./shortcuts";

const ANYWHERE = { onButton: false, onSlider: false };

describe("shortcutFor", () => {
  it("maps the usual keys", () => {
    expect(shortcutFor("k", ANYWHERE)).toEqual({ type: "toggle-play" });
    expect(shortcutFor("ArrowLeft", ANYWHERE)).toEqual({
      seconds: -5,
      type: "seek-by",
    });
    expect(shortcutFor("l", ANYWHERE)).toEqual({
      seconds: 10,
      type: "seek-by",
    });
    expect(shortcutFor("ArrowUp", ANYWHERE)).toEqual({
      delta: 0.1,
      type: "volume-by",
    });
  });

  it("ignores the case of letters", () => {
    expect(shortcutFor("F", ANYWHERE)).toEqual({ type: "toggle-fullscreen" });
    expect(shortcutFor("M", ANYWHERE)).toEqual({ type: "toggle-mute" });
  });

  it("jumps to a tenth of the way per digit", () => {
    expect(shortcutFor("0", ANYWHERE)).toEqual({
      fraction: 0,
      type: "seek-to",
    });
    expect(shortcutFor("7", ANYWHERE)).toEqual({
      fraction: 0.7,
      type: "seek-to",
    });
  });

  it("leaves Space to a focused button", () => {
    expect(shortcutFor(" ", { onButton: true, onSlider: false })).toBeNull();
    expect(shortcutFor("k", { onButton: true, onSlider: false })).toEqual({
      type: "toggle-play",
    });
  });

  it("leaves the arrows, Home and End to a focused slider", () => {
    const onSlider = { onButton: false, onSlider: true };
    expect(shortcutFor("ArrowRight", onSlider)).toBeNull();
    expect(shortcutFor("Home", onSlider)).toBeNull();
    expect(shortcutFor(" ", onSlider)).toEqual({ type: "toggle-play" });
  });

  it("does nothing for other keys", () => {
    expect(shortcutFor("Tab", ANYWHERE)).toBeNull();
    expect(shortcutFor("x", ANYWHERE)).toBeNull();
  });
});
