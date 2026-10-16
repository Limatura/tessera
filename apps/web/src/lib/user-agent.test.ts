import { describe, expect, it } from "vite-plus/test";

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
  it("no UA, no answer", () => {
    expect(parseUserAgent(null)).toBeNull();
    expect(parseUserAgent()).toBeNull();
    expect(parseUserAgent("")).toBeNull();
  });

  it("returns null for a string it cannot read rather than guessing", () => {
    expect(parseUserAgent("ProbablyNotABrowser/1.0")).toBeNull();
  });

  it("Edge is not taken for Chrome or Safari", () => {
    expect(parseUserAgent(UA.edgeWindows)).toEqual({
      browser: "Edge",
      os: "Windows",
    });
  });

  it("Chrome is not overridden by the Safari in its own UA", () => {
    expect(parseUserAgent(UA.chromeWindows)).toEqual({
      browser: "Chrome",
      os: "Windows",
    });
  });

  it("Chrome on Android is not read as Linux", () => {
    expect(parseUserAgent(UA.chromeAndroid)).toEqual({
      browser: "Chrome",
      os: "Android",
    });
  });

  it("the Mac OS X in an iPad's UA is not read as macOS", () => {
    expect(parseUserAgent(UA.safariIpad)).toEqual({
      browser: "Safari",
      os: "iOS",
    });
  });

  it("real Safari is recognised", () => {
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

  it("recognising half of it still counts", () => {
    expect(parseUserAgent(UA.curl)).toEqual({ browser: "curl", os: null });
  });
});

describe("formatUserAgent", () => {
  it("joins both halves with a middle dot", () => {
    expect(formatUserAgent(UA.safariMac)).toBe("Safari · macOS");
  });

  it("with only one half, no empty separator is left", () => {
    expect(formatUserAgent(UA.curl)).toBe("curl");
  });

  it("returns null when unrecognised, so the page falls back to the raw string", () => {
    expect(formatUserAgent("ProbablyNotABrowser/1.0")).toBeNull();
  });
});
