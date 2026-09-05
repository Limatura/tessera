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
  // *.d.ts excluded: oxlint (part of `ultracite fix`) never lints declaration
  // files, so a commit touching only *.d.ts would give it an empty target
  // list and it'd exit 1 with "No files found to lint", aborting the hook.
  //
  // apps/server, apps/docs, packages/ui, and the agent-config dirs are all in
  // oxlint.config.ts's ignorePatterns (generated/vendored code we don't lint).
  // Same failure mode as *.d.ts: if every staged file falls under one of
  // these, oxlint's target list is empty and it exits 1, aborting the hook.
  // A glob can't express "excluded dir" cleanly across multiple path
  // segments, so filter in JS and no-op when nothing is left to lint.
  "*.{js,jsx,mjs,cjs,ts,tsx,mts,cts}": (files) => {
    const ignoredDirs = [
      "apps/server/",
      "apps/docs/",
      "packages/ui/",
      ".agents/",
      ".claude/",
      ".codex/",
      ".roo/",
    ];
    const targets = files.filter(
      (file) =>
        !file.endsWith(".d.ts") &&
        !ignoredDirs.some(
          (dir) => file.includes(`/${dir}`) || file.startsWith(dir)
        )
    );
    return targets.length > 0
      ? [`ultracite fix ${targets.join(" ")}`]
      : ["true"];
  },
};
// oxlint (part of `ultracite fix`) doesn't lint CSS, so a commit touching
// only CSS files makes it exit 1 with "No files found to lint" and abort
// the whole hook. Format CSS with oxfmt directly instead.
