export default {
  "*.css": ["oxfmt --write"],
  "*.rs": ["cargo +nightly fmt --"],
  // *.d.ts excluded: oxlint (part of `ultracite fix`) never lints declaration
  // files, so a commit touching only *.d.ts would give it an empty target
  // list and it'd exit 1 with "No files found to lint", aborting the hook.
  "!(*.d).{js,jsx,mjs,cjs,ts,tsx,mts,cts}": ["ultracite fix"],
};
// oxlint (part of `ultracite fix`) doesn't lint CSS, so a commit touching
// only CSS files makes it exit 1 with "No files found to lint" and abort
// the whole hook. Format CSS with oxfmt directly instead.
