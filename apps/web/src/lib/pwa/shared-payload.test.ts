import { describe, expect, it } from "vite-plus/test";

import type { SharedPayload } from "./shared-payload";
import { draftFromPayload } from "./shared-payload";

const empty: SharedPayload = { files: [], text: "", title: "", url: "" };

describe("draftFromPayload", () => {
  it("puts files in the file tab, ignoring a caption", () => {
    const file = new File(["x"], "a.png");
    expect(
      draftFromPayload({ ...empty, files: [file], text: "look at this" })
    ).toEqual({ files: [file], kind: "file" });
  });

  it("treats a lone url as a link", () => {
    expect(draftFromPayload({ ...empty, url: "https://a.example" })).toEqual({
      kind: "link",
      url: "https://a.example",
    });
  });

  it("treats text that is only a url as a link", () => {
    expect(
      draftFromPayload({ ...empty, text: " https://a.example/x " })
    ).toEqual({ kind: "link", url: "https://a.example/x" });
  });

  it("keeps text with a url as text, url on its own line", () => {
    expect(
      draftFromPayload({
        ...empty,
        text: "read this",
        title: "Article",
        url: "https://a.example",
      })
    ).toEqual({
      content: "read this\nhttps://a.example",
      kind: "code",
      title: "Article",
    });
  });

  it("does not take a non-http scheme for a link", () => {
    expect(draftFromPayload({ ...empty, text: "ftp://a.example/x" })).toEqual({
      content: "ftp://a.example/x",
      kind: "code",
    });
  });

  it("returns null for an empty share", () => {
    expect(draftFromPayload(empty)).toBeNull();
  });
});
