#!/usr/bin/env bash
#
# Puts a running instance into a state Schemathesis can actually explore, and
# prints an API token on stdout for the authenticated pass.
#
# Without this, a third of the surface answers 404 to everything: `/shares/{code}`
# takes a code that has to exist, and no generator invents a valid one — the
# codes are random. Schemathesis says so itself ("Missing test data: 3 operations
# repeatedly returned 404, preventing tests from reaching your API's core logic").
#
# Prints `token=<secret>` and `code=<retrieval code>`, one per line, for the
# caller to fold into the Schemathesis config.
#
# Usage: fuzz-seed.sh <base-url> <server-log>   # e.g. http://127.0.0.1:13999
set -euo pipefail

BASE="${1:?usage: fuzz-seed.sh <base-url> <server-log>}"
LOG="${2:?usage: fuzz-seed.sh <base-url> <server-log>}"
API="$BASE/api/v1"
JAR=$(mktemp)
trap 'rm -f "$JAR"' EXIT

# The first run generates an administrator and prints its password once, to the
# log. Stripping ANSI first: the pretty formatter colours the field, so a plain
# grep for the value picks up escape codes with it.
BOOTSTRAP=$(sed 's/\x1b\[[0-9;]*m//g' "$LOG" | grep -o 'password=[a-f0-9]\{64\}' | head -1 | cut -d= -f2)
if [ -z "$BOOTSTRAP" ]; then
  echo "::error::no bootstrap password in $LOG — did the server start with a fresh database?" >&2
  exit 1
fi

fail_unless() {
  # $1 expected, $2 actual, $3 what
  if [ "$2" != "$1" ]; then
    echo "::error::$3 returned $2, expected $1" >&2
    exit 1
  fi
}

code=$(curl -s -c "$JAR" -o /dev/null -w '%{http_code}' -X POST "$API/auth/login" \
  -H 'content-type: application/json' \
  -d "{\"identifier\":\"admin\",\"password\":\"$BOOTSTRAP\"}")
fail_unless 200 "$code" "login"

# The bootstrapped administrator carries `must_change_password`, and the strict
# extractor refuses every other endpoint until it is cleared. So this is not
# tidiness — without it the token below cannot be issued.
code=$(curl -s -b "$JAR" -c "$JAR" -o /dev/null -w '%{http_code}' -X PATCH "$API/auth/password" \
  -H 'content-type: application/json' \
  -d "{\"current_password\":\"$BOOTSTRAP\",\"new_password\":\"schemathesis-seed-password\"}")
fail_unless 200 "$code" "change password"

# Every permission this version has, asked for rather than written down: a
# hand-kept list here goes stale the moment one is added, and it goes stale
# silently — the token keeps working, just not for the new endpoint.
#
# Without this the token carries the mint-time default, which is `shares:create`
# and nothing else. Measured before it was asked for: `GET /admin/roles`,
# `GET /admin/users` and even `GET /tokens` all answered 403, so every
# authenticated operation in the run was exercising its refusal path and
# nothing else.
PERMISSIONS=$(curl -s -b "$JAR" "$API/admin/permissions" \
  | python3 -c 'import json,sys; print(json.dumps([p["name"] for p in json.load(sys.stdin)]))')
if [ -z "$PERMISSIONS" ] || [ "$PERMISSIONS" = "[]" ]; then
  echo "::error::could not read the permission list from $API/admin/permissions" >&2
  exit 1
fi

token_body=$(curl -s -b "$JAR" -X POST "$API/tokens" \
  -H 'content-type: application/json' \
  -d "{\"name\":\"schemathesis\",\"permissions\":$PERMISSIONS}")
TOKEN=$(printf '%s' "$token_body" | python3 -c 'import json,sys; print(json.load(sys.stdin)["secret"])')

# One share, so `/shares/{code}` and `/unlock` have something real to answer
# with. No `filename` query parameter, which is what makes it a note rather than
# a file — the kind is derived from that, not declared.
share=$(curl -s -X POST "$API/shares" \
  -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/octet-stream' \
  --data-binary 'schemathesis seed content')
CODE=$(printf '%s' "$share" | python3 -c 'import json,sys; print(json.load(sys.stdin)["code"])')

echo "seeded: share $CODE, token ${TOKEN:0:8}…" >&2
printf 'token=%s\ncode=%s\n' "$TOKEN" "$CODE"
