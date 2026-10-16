#!/usr/bin/env bash
# The version step of a release: what `changesets/action` runs as its
# `version-script`. A file, not a command line, because the action splits that
# input on spaces itself and never hands it to a shell.
set -euo pipefail

cd "$(dirname "$0")/.."

pnpm changeset version
./scripts/sync-cargo-version.sh

# Every app in the fixed group keeps its own CHANGELOG.md, duplicates included:
# the action reads each bumped package's changelog for the PR description and
# fails on a missing one.
