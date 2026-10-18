import { describe, expect, it } from "vite-plus/test";

import { formatTime } from "./format-time";

describe("formatTime", () => {
  it("shows minutes and seconds under an hour", () => {
    expect(formatTime(0)).toBe("0:00");
    expect(formatTime(5.9)).toBe("0:05");
    expect(formatTime(754)).toBe("12:34");
  });

  it("adds hours from an hour up", () => {
    expect(formatTime(3600)).toBe("1:00:00");
    expect(formatTime(3725)).toBe("1:02:05");
  });

  it("matches the shape of the reference", () => {
    expect(formatTime(65, 4000)).toBe("0:01:05");
  });

  it("reads anything that is not a time as zero", () => {
    expect(formatTime(Number.NaN)).toBe("0:00");
    expect(formatTime(Number.POSITIVE_INFINITY)).toBe("0:00");
    expect(formatTime(-3)).toBe("0:00");
  });
});
