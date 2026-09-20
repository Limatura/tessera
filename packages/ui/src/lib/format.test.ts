import { describe, expect, it } from "vitest";

import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("0 和負數都顯示 0 B", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-1)).toBe("0 B");
  });

  it("位元組不加小數點", () => {
    expect(formatBytes(1)).toBe("1 B");
    expect(formatBytes(999)).toBe("999 B");
  });

  it("KB 以上保留一位小數", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 ** 2)).toBe("1.0 MB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
  });

  it("超過最大單位就停在 TB,不會跑出未定義的單位", () => {
    expect(formatBytes(1024 ** 4)).toBe("1.0 TB");
    expect(formatBytes(1024 ** 6)).toBe("1048576.0 TB");
  });
});
