import { describe, expect, it } from "vite-plus/test";

import { hasDecodedAudio, hasPicture } from "./media-diagnosis";

describe("hasPicture", () => {
  it("needs a decoded width", () => {
    expect(hasPicture({ videoWidth: 1920 })).toBe(true);
    expect(hasPicture({ videoWidth: 0 })).toBe(false);
  });
});

describe("hasDecodedAudio", () => {
  it("reads Firefox's flag", () => {
    expect(hasDecodedAudio({ mozHasAudio: false, videoWidth: 1 })).toBe(false);
    expect(hasDecodedAudio({ mozHasAudio: true, videoWidth: 1 })).toBe(true);
  });

  it("reads Chromium's byte counter", () => {
    expect(
      hasDecodedAudio({ videoWidth: 1, webkitAudioDecodedByteCount: 0 })
    ).toBe(false);
    expect(
      hasDecodedAudio({ videoWidth: 1, webkitAudioDecodedByteCount: 4096 })
    ).toBe(true);
  });

  it("falls back to Safari's track list", () => {
    expect(hasDecodedAudio({ audioTracks: { length: 0 }, videoWidth: 1 })).toBe(
      false
    );
    expect(hasDecodedAudio({ audioTracks: { length: 2 }, videoWidth: 1 })).toBe(
      true
    );
  });

  it("does not guess when the browser cannot say", () => {
    expect(hasDecodedAudio({ videoWidth: 1 })).toBeNull();
  });
});
