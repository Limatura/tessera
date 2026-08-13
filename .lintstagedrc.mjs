export default {
  "*.rs": ["cargo +nightly fmt --"],
  "*.{js,jsx,mjs,cjs,ts,tsx,mts,cts,css}": (files) => {
    const targets = files.filter((file) => !file.includes("/packages/ui/"));

    if (targets.length === 0) {
      return [];
    }

    return [`ultracite fix ${targets.join(" ")}`];
  },
};
