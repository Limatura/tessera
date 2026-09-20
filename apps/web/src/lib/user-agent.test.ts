import { describe, expect, it } from "vitest";

import { formatUserAgent, parseUserAgent } from "./user-agent";

const UA = {
  chromeWindows:
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
  chromeAndroid:
    "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36",
  curl: "curl/8.4.0",
  edgeWindows:
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0",
  firefoxLinux:
    "Mozilla/5.0 (X11; Linux x86_64; rv:127.0) Gecko/20100101 Firefox/127.0",
  safariIpad:
    "Mozilla/5.0 (iPad; CPU OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1",
  safariMac:
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15",
};

describe("parseUserAgent", () => {
  it("沒有 UA 就沒有答案", () => {
    expect(parseUserAgent(null)).toBeNull();
    expect(parseUserAgent()).toBeNull();
    expect(parseUserAgent("")).toBeNull();
  });

  it("認不出來的字串回 null,不硬湊", () => {
    expect(parseUserAgent("ProbablyNotABrowser/1.0")).toBeNull();
  });

  it("Edge 不會被當成 Chrome 或 Safari", () => {
    expect(parseUserAgent(UA.edgeWindows)).toEqual({
      browser: "Edge",
      os: "Windows",
    });
  });

  it("Chrome 不會被自己 UA 裡的 Safari 蓋掉", () => {
    expect(parseUserAgent(UA.chromeWindows)).toEqual({
      browser: "Chrome",
      os: "Windows",
    });
  });

  it("Android 上的 Chrome 不會被認成 Linux", () => {
    expect(parseUserAgent(UA.chromeAndroid)).toEqual({
      browser: "Chrome",
      os: "Android",
    });
  });

  it("iPad 的 Mac OS X 不會被認成 macOS", () => {
    expect(parseUserAgent(UA.safariIpad)).toEqual({
      browser: "Safari",
      os: "iOS",
    });
  });

  it("真的 Safari 認得出來", () => {
    expect(parseUserAgent(UA.safariMac)).toEqual({
      browser: "Safari",
      os: "macOS",
    });
  });

  it("Firefox on Linux", () => {
    expect(parseUserAgent(UA.firefoxLinux)).toEqual({
      browser: "Firefox",
      os: "Linux",
    });
  });

  it("只認出一半也算數", () => {
    expect(parseUserAgent(UA.curl)).toEqual({ browser: "curl", os: null });
  });
});

describe("formatUserAgent", () => {
  it("兩邊都有就用中點串起來", () => {
    expect(formatUserAgent(UA.safariMac)).toBe("Safari · macOS");
  });

  it("只有一邊就不留空的分隔符", () => {
    expect(formatUserAgent(UA.curl)).toBe("curl");
  });

  it("認不出來回 null,讓畫面退回原始字串", () => {
    expect(formatUserAgent("ProbablyNotABrowser/1.0")).toBeNull();
  });
});
