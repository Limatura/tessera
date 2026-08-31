export default {
  // `docs/schema` is generated from the migrations, and CI fails when it
  // drifts. Regenerating it here means the drift is fixed before the commit
  // rather than twenty minutes later on a runner that has one job slot.
  //
  // Only when a migration or the notes change, because this starts a container
  // and that is not a thing to do on every commit. A machine without docker is
  // told and let through: CI is still the backstop, and blocking somebody's
  // commit over a tool they do not have is the worse failure.
  //
  // No `git add`: lint-staged stages what its tasks write, and doing it here
  // as well makes it warn.
  "{apps/server/src/db/migration/sql/**/*.sql,.tbls.yml}": () =>
    "sh -c 'command -v docker >/dev/null 2>&1 || { echo \"docs/schema not regenerated: no docker on this machine; CI will check it\"; exit 0; }; python3 scripts/schema-docs.py'",
  "*.css": ["oxfmt --write"],
  "*.rs": ["cargo +nightly fmt --"],
  "*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}": ["ultracite fix"],
};
// oxlint (part of `ultracite fix`) doesn't lint CSS, so a commit touching
// only CSS files makes it exit 1 with "No files found to lint" and abort
// the whole hook. Format CSS with oxfmt directly instead.
