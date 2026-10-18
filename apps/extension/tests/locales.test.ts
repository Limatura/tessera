/**
 * The translations against each other and against the code.
 *
 * `browser.i18n` never fails on a missing translation: it shows the default
 * locale's (zh_TW) instead, silently. So a key added to one file only, a
 * `$2` dropped in translation, or a key nothing uses any more is only ever
 * caught here.
 */
import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";

import { parseMessagesFile } from "@wxt-dev/i18n/build";
import type { ParsedMessage } from "@wxt-dev/i18n/build";
import { describe, expect, it } from "vite-plus/test";

const ROOT = path.resolve(import.meta.dirname, "..");
const LOCALES = path.join(ROOT, "locales");
const DEFAULT_LOCALE = "zh_TW";
/** Where keys are used: the code, and the manifest's `__MSG_…__`. */
const SOURCES = ["entrypoints", "lib", "components", "hooks"];

/** A message's shape: what has to match across locales. */
function shapes(messages: ParsedMessage[]): Map<string, string> {
  return new Map(
    messages.map((message) => [
      message.key.join("."),
      `${message.type}, ${message.substitutions} substitution(s)`,
    ])
  );
}

async function load(locale: string): Promise<Map<string, string>> {
  return shapes(await parseMessagesFile(path.join(LOCALES, `${locale}.yml`)));
}

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && /\.tsx?$/u.test(entry.name))
    .map((entry) => path.join(entry.parentPath, entry.name));
}

const locales = readdirSync(LOCALES)
  .filter((file) => file.endsWith(".yml"))
  .map((file) => file.replace(/\.yml$/u, ""))
  .filter((locale) => locale !== DEFAULT_LOCALE);

describe("translations", () => {
  it.each(locales)(
    "%s has every key the default has, in the same shape, and no others",
    async (locale) => {
      const expected = await load(DEFAULT_LOCALE);
      const actual = await load(locale);
      expect(Object.fromEntries(actual)).toEqual(Object.fromEntries(expected));
    }
  );

  it("has no empty message", async () => {
    const messages = await parseMessagesFile(
      path.join(LOCALES, `${DEFAULT_LOCALE}.yml`)
    );
    const empty = messages
      .filter((message) =>
        message.type === "plural"
          ? Object.values(message.plurals).some((form) => !form.trim())
          : !message.message.trim()
      )
      .map((message) => message.key.join("."));
    expect(empty).toEqual([]);
  });

  it("uses every key it defines, in the code or the manifest", async () => {
    const code = [
      ...SOURCES.flatMap((directory) =>
        sourceFiles(path.join(ROOT, directory))
      ),
      path.join(ROOT, "wxt.config.ts"),
    ]
      .map((file) => readFileSync(file, "utf-8"))
      .join("\n");
    const used = new Set([
      ...[...code.matchAll(/i18n\.t\(\s*"(?<key>[\w.]+)"/gu)].map(
        (match) => match.groups?.key
      ),
      // The manifest flattens a key's dots into underscores.
      ...[...code.matchAll(/__MSG_(?<key>\w+)__/gu)].map(
        (match) => match.groups?.key
      ),
    ]);
    // `@@ui_locale` and the like are the browser's own, added by the parser.
    const defaults = await load(DEFAULT_LOCALE);
    const keys = [...defaults.keys()].filter((key) => !key.startsWith("@@"));
    const unused = keys.filter(
      (key) => !used.has(key) && !used.has(key.replaceAll(".", "_"))
    );
    expect(unused).toEqual([]);
  });
});
