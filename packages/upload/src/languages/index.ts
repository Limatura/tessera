/**
 * The languages a note can be marked as, shared by every client that makes
 * one, so a web page's code block and the site's code tab agree on names.
 *
 * Adding a language touches these places, and missing one fails silently:
 *
 * 1. `LANGUAGES` in `detect.ts`: guessing it from the text
 * 2. `LANGUAGE_LABELS` in `labels.ts`: its name, and the site's select
 * 3. `LANGUAGE_ALIASES` in `aliases.ts`: the names other tools give it
 * 4. `bundledLanguages` in apps/web `lib/shiki.ts`: highlighting on the share
 *    page, which otherwise falls back to plain text
 */
export {
  languageFromClassName,
  languageFromPath,
  normalizeLanguage,
} from "./aliases";
export { detectLanguage } from "./detect";
export { LANGUAGE_LABELS, languageLabel } from "./labels";
