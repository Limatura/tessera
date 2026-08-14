export default {
  "*.rs": ["cargo +nightly fmt --"],
  "*.{js,jsx,mjs,cjs,ts,tsx,mts,cts,css}": ["ultracite fix"],
};
