import { describe, expect, it } from "vite-plus/test";

import {
  MAX_HELD_ENTRIES,
  appendOlder,
  compareEntryIds,
  mergeNewer,
} from "./log-entries";
import type { LogEntry } from "./log-entries";

function entry(id: string): LogEntry {
  return {
    fields: {},
    id,
    level: "info",
    message: id,
    target: "test",
    time: "2026-10-19T00:00:00.000Z",
  };
}

const ids = (entries: LogEntry[]) => entries.map((item) => item.id);

describe("compareEntryIds", () => {
  it("orders offsets as numbers", () => {
    expect(compareEntryIds("2026-10-19:9", "2026-10-19:10")).toBeLessThan(0);
  });

  it("orders by day before offset", () => {
    expect(compareEntryIds("2026-10-18:900", "2026-10-19:1")).toBeLessThan(0);
  });
});

describe("mergeNewer", () => {
  it("puts a batch, sent oldest first, in front of the list", () => {
    const merged = mergeNewer(
      [entry("2026-10-19:1")],
      [entry("2026-10-19:2"), entry("2026-10-19:3")]
    );
    expect(ids(merged)).toEqual([
      "2026-10-19:3",
      "2026-10-19:2",
      "2026-10-19:1",
    ]);
  });

  it("skips lines it already holds and keeps newest first", () => {
    const merged = mergeNewer(
      [entry("2026-10-19:5"), entry("2026-10-19:1")],
      [entry("2026-10-19:3"), entry("2026-10-19:5"), entry("2026-10-19:7")]
    );
    expect(ids(merged)).toEqual([
      "2026-10-19:7",
      "2026-10-19:5",
      "2026-10-19:3",
      "2026-10-19:1",
    ]);
  });

  it("drops the oldest lines past the cap", () => {
    const held = Array.from({ length: MAX_HELD_ENTRIES }, (_, index) =>
      entry(`2026-10-19:${MAX_HELD_ENTRIES - index}`)
    );
    const merged = mergeNewer(held, [entry("2026-10-19:99999")]);
    expect(merged).toHaveLength(MAX_HELD_ENTRIES);
    expect(merged[0]?.id).toBe("2026-10-19:99999");
    expect(merged.at(-1)?.id).toBe("2026-10-19:2");
  });
});

describe("appendOlder", () => {
  it("puts older lines after the list without repeating one", () => {
    const merged = appendOlder(
      [entry("2026-10-19:5")],
      [entry("2026-10-19:5"), entry("2026-10-19:1")]
    );
    expect(ids(merged)).toEqual(["2026-10-19:5", "2026-10-19:1"]);
  });
});
