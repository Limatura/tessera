#!/usr/bin/env bash
# Post a comment on a pull request, or edit the one this workflow left last
# time. Every report goes to the same comment: a size table appended once per
# push turns a ten-commit branch into ten stale tables, and the only one worth
# reading is the last.
#
# curl and python3 rather than `gh` or `actions/github-script`: python3 is
# already what the schema check runs on this runner, `gh` is not guaranteed to
# be installed on a self-hosted one, and a ninth third-party action in this
# workflow would need a pin, a review and a reason.
#
# Everything variable arrives through the environment, never through `${{ }}`
# interpolated into the script — a body containing a backtick would otherwise
# be a command.

set -euo pipefail

: "${GITHUB_TOKEN:?a token with pull-requests: write}"
: "${REPO:?owner/name}"
: "${PR_NUMBER:?}"
: "${MARKER:?an HTML comment identifying which comment to reuse}"
: "${BODY:?}"

API=${GITHUB_API_URL:-https://api.github.com}

api() {
  curl --fail-with-body -sS \
    -H "Authorization: Bearer $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github+json" \
    -H "Content-Type: application/json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "$@"
}

# One page. The comment this looks for is created the first time the workflow
# runs on the branch and issue comments come back oldest first, so it stays on
# page one for any pull request that is not already a hundred comments long
# before its first build.
existing=$(
  api "$API/repos/$REPO/issues/$PR_NUMBER/comments?per_page=100" \
    | python3 -c '
import json, os, sys

marker = os.environ["MARKER"]
for comment in json.load(sys.stdin):
    if marker in (comment.get("body") or ""):
        print(comment["id"])
        break
'
)

payload=$(python3 -c '
import json, os

print(json.dumps({"body": os.environ["MARKER"] + "\n" + os.environ["BODY"]}))
')

if [ -n "$existing" ]; then
  api -X PATCH "$API/repos/$REPO/issues/comments/$existing" -d "$payload" > /dev/null
  echo "updated comment $existing"
else
  api -X POST "$API/repos/$REPO/issues/$PR_NUMBER/comments" -d "$payload" > /dev/null
  echo "posted a new comment"
fi
