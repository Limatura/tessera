import { describe, expect, it } from "vite-plus/test";

import {
  gaugeFraction,
  median,
  randomBytes,
  toMbps,
  windowedMbps,
} from "./speed-test";

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

describe("gaugeFraction", () => {
  it("spaces the stops evenly", () => {
    expect(gaugeFraction(0)).toBe(0);
    expect(gaugeFraction(10)).toBeCloseTo(2 / 7);
    expect(gaugeFraction(1000)).toBe(1);
    expect(gaugeFraction(5000)).toBe(1);
    expect(gaugeFraction(-3)).toBe(0);
  });

  it("is monotonic", () => {
    expect(gaugeFraction(75)).toBeGreaterThan(gaugeFraction(50));
    expect(gaugeFraction(75)).toBeLessThan(gaugeFraction(100));
  });
});

describe("windowedMbps", () => {
  it("uses only the recent window", () => {
    const samples = [
      { at: 0, bytes: 0 },
      { at: 1000, bytes: 1_000_000 },
      { at: 1500, bytes: 6_000_000 },
    ];
    // 5 MB in the last 500 ms = 80 Mbps.
    expect(windowedMbps(samples, 400)).toBeCloseTo(80);
  });

  it("is zero with nothing to compare", () => {
    expect(windowedMbps([])).toBe(0);
    expect(windowedMbps([{ at: 0, bytes: 1 }])).toBe(0);
  });
});
