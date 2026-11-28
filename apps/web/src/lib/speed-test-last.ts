/**
 * The last finished speed test in this browser, so the bandwidth settings can
 * suggest limits from it. Per browser on purpose: the reading is of the path
 * from this browser, and only means something to whoever ran it.
 */

const STORAGE_KEY = "tessera.speed-test.last";

export interface LastSpeedTest {
  downloadMbps: number;
  uploadMbps: number;
  /** When it finished, as epoch milliseconds. */
  at: number;
}

export function saveLastSpeedTest(result: LastSpeedTest): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(result));
  } catch {
    // Storage blocked or full: there is simply no suggestion later.
  }
}

export function loadLastSpeedTest(): LastSpeedTest | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return null;
    }
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "downloadMbps" in parsed &&
      "uploadMbps" in parsed &&
      "at" in parsed &&
      typeof parsed.downloadMbps === "number" &&
      typeof parsed.uploadMbps === "number" &&
      typeof parsed.at === "number"
    ) {
      return {
        at: parsed.at,
        downloadMbps: parsed.downloadMbps,
        uploadMbps: parsed.uploadMbps,
      };
    }
    return null;
  } catch {
    return null;
  }
}
