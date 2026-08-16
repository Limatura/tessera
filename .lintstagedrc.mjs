export default {
  "*.css": ["oxfmt --write"],
  "*.rs": ["cargo +nightly fmt --"],
  "*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}": ["ultracite fix"],
};
// oxlint (part of `ultracite fix`) doesn't lint CSS, so a commit touching
// only CSS files makes it exit 1 with "No files found to lint" and abort
// the whole hook. Format CSS with oxfmt directly instead.
