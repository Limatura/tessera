import { describe, expect, it } from "vite-plus/test";

import { bytesToMbps, mbpsToBytes, suggestedMbps } from "./bandwidth";

describe("bandwidth units", () => {
  it("converts between Mbit/s and bytes per second", () => {
    expect(mbpsToBytes(8)).toBe(1_000_000);
    expect(bytesToMbps(1_000_000)).toBe(8);
    expect(mbpsToBytes(0)).toBe(0);
  });

  it("suggests half the measured speed", () => {
    expect(suggestedMbps(100)).toBe(50);
    expect(suggestedMbps(37.3)).toBe(18.7);
  });

  it("never suggests a value the server refuses", () => {
    expect(mbpsToBytes(suggestedMbps(0.001))).toBeGreaterThanOrEqual(1024);
  });
});
