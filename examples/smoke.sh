#!/usr/bin/env bash
# Exercise the Anvaya daemon against a fresh workspace.
#
# Usage:
#   ANVAYA_WORKSPACE=/tmp/anvaya-demo ./examples/smoke.sh
# (assumes the daemon is already running on $BASE)

set -euo pipefail

BASE="${ANVAYA_BASE:-http://127.0.0.1:7878}"
CT='-H content-type:application/json'

post() {
  echo
  echo "== $1 =="
  curl -s -X POST "$BASE$1" $CT -d "$2"; echo
}

post /api/v1/mkdir       '{"path":"demo"}'
post /api/v1/write       '{"path":"demo/greeting.txt","content":"hello from anvaya\n"}'
post /api/v1/read        '{"path":"demo/greeting.txt"}'
post /api/v1/list        '{"path":"demo"}'
post /api/v1/copy        '{"src":"demo/greeting.txt","dst":"demo/copy.txt"}'
post /api/v1/move        '{"src":"demo/copy.txt","dst":"demo/renamed.txt"}'
post /api/v1/delete      '{"path":"demo/renamed.txt"}'

# Expected to be rejected with code "path_traversal"
post /api/v1/read        '{"path":"../../../etc/passwd"}'