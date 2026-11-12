import { describe, expect, it } from "vite-plus/test";

import { median, randomBytes, toMbps } from "./speed-test";

describe("toMbps", () => {
  it("uses decimal megabits", () => {
    expect(toMbps(1_250_000, 1)).toBe(10);
  });

  it("reports nothing for no elapsed time", () => {
    expect(toMbps(1000, 0)).toBe(0);
  });
});

describe("median", () => {
  it("takes the middle of an odd count", () => {
    expect(median([30, 10, 20])).toBe(20);
  });

  it("averages the middle pair of an even count", () => {
    expect(median([40, 10, 20, 30])).toBe(25);
  });

  it("is zero for no samples", () => {
    expect(median([])).toBe(0);
  });
});

describe("randomBytes", () => {
  it("fills past the getRandomValues limit", () => {
    const bytes = randomBytes(200_000);
    expect(bytes.byteLength).toBe(200_000);
    expect(bytes.subarray(150_000).some((byte) => byte !== 0)).toBe(true);
  });
});
