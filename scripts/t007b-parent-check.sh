#!/bin/zsh
# Parent NORMAL proof only. No native launch, account call, hook execution or owner settings.
set -eu
cd /Volumes/MeshyardProjects/meshyard-worktrees/t-007b-local-agent-integration
source /Volumes/MeshyardProjects/meshyard-smithers/env.sh
export CARGO_NET_OFFLINE=true
export MESHYARD_CHECK_CONTEXT='parent NORMAL authorized shell; USB-only file-key tests; no owner config or native acceptance'
if [[ "$#" != 2 || "$1" != --output-dir ]]; then
  print -u2 'Usage: zsh scripts/t007b-parent-check.sh --output-dir /Volumes/MeshyardProjects/Meshyard-build/orchestration/t007b-connect-evidence/FRESH_NAME'
  exit 2
fi
output="$2"
[[ "$output" == /Volumes/MeshyardProjects/Meshyard-build/orchestration/t007b-connect-evidence/* ]]
mkdir "$output"
python3 scripts/t007b-record-check.py --output-dir "$output/uds" cargo test --offline -p meshyard-app uds_ -- --nocapture
python3 scripts/t007b-record-check.py --output-dir "$output/full-check" npm run check
python3 scripts/t007b-record-check.py --output-dir "$output/browser-command" node scripts/t007b-connect-qa.mjs --output-dir "$output/browser"
