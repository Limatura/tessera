import { describe, expect, it } from "vite-plus/test";

import { fromDisplay, pickUnit, toDisplay, unitsFor } from "./units";

describe("unitsFor", () => {
  it("offers nothing finer than the stored unit", () => {
    expect(unitsFor("seconds")).toEqual(["second", "minute", "hour", "day"]);
    expect(unitsFor("hours")).toEqual(["hour", "day"]);
    expect(unitsFor("days")).toEqual(["day"]);
  });

  it("offers KB up for both byte and KiB settings", () => {
    expect(unitsFor("bytes")).toEqual(["KB", "MB", "GB", "TB"]);
    expect(unitsFor("kib")).toEqual(["KB", "MB", "GB"]);
  });
});

describe("pickUnit", () => {
  it("picks the largest unit the value is a whole number of", () => {
    expect(pickUnit(900, "seconds")).toBe("minute");
    expect(pickUnit(3600, "seconds")).toBe("hour");
    expect(pickUnit(168, "hours")).toBe("day");
    expect(pickUnit(2 * 1024 ** 3, "bytes")).toBe("GB");
    expect(pickUnit(65_536, "kib")).toBe("MB");
  });

  it("falls back to the smallest unit for uneven or empty values", () => {
    expect(pickUnit(90, "seconds")).toBe("second");
    expect(pickUnit(1_500_000, "bytes")).toBe("KB");
    expect(pickUnit(65_536, "bytes")).toBe("KB");
    expect(pickUnit(0, "hours")).toBe("hour");
    expect(pickUnit(undefined, "bytes")).toBe("KB");
    expect(pickUnit(Number.NaN, "seconds")).toBe("second");
  });
});

describe("toDisplay / fromDisplay", () => {
  it("round-trips whole amounts", () => {
    expect(toDisplay(900, "seconds", "minute")).toBe(15);
    expect(fromDisplay(15, "seconds", "minute")).toBe(900);
    expect(toDisplay(2 * 1024 ** 3, "bytes", "GB")).toBe(2);
    expect(fromDisplay(2, "bytes", "GB")).toBe(2 * 1024 ** 3);
  });

  it("keeps the amount when the unit changes", () => {
    expect(toDisplay(900, "seconds", "second")).toBe(900);
    expect(toDisplay(36, "hours", "day")).toBe(1.5);
  });

  it("rounds a fractional input to a whole stored unit", () => {
    expect(fromDisplay(1.5, "bytes", "GB")).toBe(1.5 * 1024 ** 3);
    expect(fromDisplay(0.5, "hours", "day")).toBe(12);
    expect(fromDisplay(0.01, "hours", "day")).toBe(0);
  });
});
