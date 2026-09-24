#!/usr/bin/env bash
# Updates the ignored tests tracking issue with test results from artifacts.
# Result parsing and markdown rendering live in render-ignored-tests-report.py;
# this script only handles the GitHub issue plumbing.
# Usage: ./update-ignored-tests-issue.sh <artifacts_dir> <run_number> <run_url>

set -euo pipefail

ARTIFACTS_DIR="${1:?Usage: $0 <artifacts_dir> <run_number> <run_url>}"
RUN_NUMBER="${2:?Usage: $0 <artifacts_dir> <run_number> <run_url>}"
RUN_URL="${3:?Usage: $0 <artifacts_dir> <run_number> <run_url>}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FAILURE_MARKER='<!-- ignored-tests-failures: '

# Extracts the machine-readable failure count embedded in an issue body.
extract_failure_count() {
  grep -oP "(?<=${FAILURE_MARKER})[0-9]+" <<< "$1" | head -n1 || echo 0
}

python3 "${SCRIPT_DIR}/render-ignored-tests-report.py" \
  "$ARTIFACTS_DIR" "$RUN_NUMBER" "$RUN_URL" > issue-body.md

total_failures=$(extract_failure_count "$(cat issue-body.md)")

issue_number=$(gh issue list --label "ignored-tests-tracker" --state open \
  --json number --jq '.[0].number // empty')

if [[ -z "$issue_number" ]]; then
  gh issue create \
    --title "Ignored Tests Status Report" \
    --body-file issue-body.md \
    --label "ignored-tests-tracker"
  echo "Created new tracking issue"
else
  previous_failures=$(extract_failure_count \
    "$(gh issue view "$issue_number" --json body --jq '.body')")

  gh issue edit "$issue_number" --body-file issue-body.md
  echo "Updated tracking issue #${issue_number}"

  if ((total_failures > previous_failures)); then
    increase=$((total_failures - previous_failures))
    gh issue comment "$issue_number" --body \
      ":warning: **Failure count increased by ${increase}** (from ${previous_failures} to ${total_failures}) in [run #${RUN_NUMBER}](${RUN_URL})"
    echo "::warning::Failure count increased from ${previous_failures} to ${total_failures}"
  fi
fi
