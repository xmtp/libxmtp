#!/usr/bin/env bash
# Updates the ignored tests tracking issue with test results from artifacts
# Usage: ./update-ignored-tests-issue.sh <artifacts_dir> <run_number> <run_url>

set -euo pipefail

ARTIFACTS_DIR="${1:?Usage: $0 <artifacts_dir> <run_number> <run_url>}"
RUN_NUMBER="${2:?Usage: $0 <artifacts_dir> <run_number> <run_url>}"
RUN_URL="${3:?Usage: $0 <artifacts_dir> <run_number> <run_url>}"

# Validate artifacts exist, create placeholders for missing ones
validate_artifacts() {
  local missing_artifacts=()
  for expected in native-default native-d14n wasm-default wasm-d14n; do
    if [[ ! -f "${ARTIFACTS_DIR}/${expected}-results/test-output.txt" ]]; then
      missing_artifacts+=("$expected")
    fi
  done

  if [[ ${#missing_artifacts[@]} -gt 0 ]]; then
    echo "::warning::Missing artifacts: ${missing_artifacts[*]}"
    for artifact in "${missing_artifacts[@]}"; do
      mkdir -p "${ARTIFACTS_DIR}/${artifact}-results"
      echo "ARTIFACT_MISSING" > "${ARTIFACTS_DIR}/${artifact}-results/test-output.txt"
    done
  fi
}

# Parse test output and extract results
# Output format: passed|failed|status|test_results_markdown
# status: ok, build_failure, missing
parse_test_output() {
  local file="$1"
  local passed=0
  local failed=0
  local status="ok"
  local test_results=""

  if [[ ! -f "$file" ]]; then
    printf '0|0|missing|'
    return
  fi

  if grep -q "ARTIFACT_MISSING" "$file"; then
    printf '0|0|missing|'
    return
  fi

  if grep -q "CARGO_BUILD_FAILURE" "$file"; then
    status="build_failure"
  fi

  while IFS= read -r line; do
    if [[ "$line" =~ ^test[[:space:]]+(.+)[[:space:]]+\.\.\.[[:space:]]+(ok|FAILED) ]]; then
      test_name="${BASH_REMATCH[1]}"
      result="${BASH_REMATCH[2]}"
      if [[ "$result" == "ok" ]]; then
        ((passed++)) || true
        test_results+="| \`${test_name}\` | :white_check_mark: ok |"$'\n'
      else
        ((failed++)) || true
        test_results+="| \`${test_name}\` | :x: FAILED |"$'\n'
      fi
    fi
  done < "$file"

  printf '%d|%d|%s|%s' "$passed" "$failed" "$status" "$test_results"
}

# Format status for display in summary table
format_status() {
  local passed="$1"
  local failed="$2"
  local status="$3"
  local total=$((passed + failed))

  case "$status" in
    missing)
      echo ":warning: Missing"
      ;;
    build_failure)
      if [[ $total -eq 0 ]]; then
        echo ":x: Build Failed"
      else
        echo "${passed} :white_check_mark: / ${failed} :x: (build issues)"
      fi
      ;;
    *)
      echo "${passed} :white_check_mark: / ${failed} :x:"
      ;;
  esac
}

# Render test results table, with message for empty results
render_test_section() {
  local total="$1"
  local status="$2"
  local tests="$3"

  if [[ "$status" == "missing" ]]; then
    echo "| *(Artifact missing - job may have failed)* | - |"
  elif [[ "$status" == "build_failure" && $total -eq 0 ]]; then
    echo "| *(Build failed before tests could run)* | - |"
  elif [[ $total -eq 0 ]]; then
    echo "| *(No ignored tests found)* | - |"
  else
    printf '%s' "$tests"
  fi
}

# Generate issue body markdown
generate_issue_body() {
  local timestamp
  timestamp=$(date -u +"%Y-%m-%d %H:%M:%S UTC")

  # Render test sections
  local native_default_section native_d14n_section wasm_default_section wasm_d14n_section
  native_default_section=$(render_test_section "$native_default_total" "$native_default_status" "$native_default_tests")
  native_d14n_section=$(render_test_section "$native_d14n_total" "$native_d14n_status" "$native_d14n_tests")
  wasm_default_section=$(render_test_section "$wasm_default_total" "$wasm_default_status" "$wasm_default_tests")
  wasm_d14n_section=$(render_test_section "$wasm_d14n_total" "$wasm_d14n_status" "$wasm_d14n_tests")

  cat << EOF
# Ignored Tests Status Report

This issue tracks the status of tests marked with \`#[ignore]\` in the codebase.
These tests are typically skipped during normal CI runs but are monitored here.

**Last Updated:** ${timestamp}
**Run:** [#${RUN_NUMBER}](${RUN_URL})

## Summary

| Target | Status | Total |
|--------|--------|-------|
| Native | ${native_default_display} | ${native_default_total} |
| Native + d14n | ${native_d14n_display} | ${native_d14n_total} |
| WASM | ${wasm_default_display} | ${wasm_default_total} |
| WASM + d14n | ${wasm_d14n_display} | ${wasm_d14n_total} |

## Native

<details>
<summary>Test Results (${native_default_total} tests)</summary>

| Test | Result |
|------|--------|
${native_default_section}
</details>

## Native + d14n

<details>
<summary>Test Results (${native_d14n_total} tests)</summary>

| Test | Result |
|------|--------|
${native_d14n_section}
</details>

## WASM

<details>
<summary>Test Results (${wasm_default_total} tests)</summary>

| Test | Result |
|------|--------|
${wasm_default_section}
</details>

## WASM + d14n

<details>
<summary>Test Results (${wasm_d14n_total} tests)</summary>

| Test | Result |
|------|--------|
${wasm_d14n_section}
</details>

---
*This issue is automatically updated by the [Ignored Tests Tracker](.github/workflows/ignored-tests-tracker.yml) workflow.*
EOF
}

main() {
  validate_artifacts

  # Parse each configuration's results
  local native_default native_d14n wasm_default wasm_d14n
  native_default=$(parse_test_output "${ARTIFACTS_DIR}/native-default-results/test-output.txt")
  native_d14n=$(parse_test_output "${ARTIFACTS_DIR}/native-d14n-results/test-output.txt")
  wasm_default=$(parse_test_output "${ARTIFACTS_DIR}/wasm-default-results/test-output.txt")
  wasm_d14n=$(parse_test_output "${ARTIFACTS_DIR}/wasm-d14n-results/test-output.txt")

  # Extract counts (format: passed|failed|status|tests)
  local native_default_passed native_default_failed native_default_status native_default_tests
  local native_d14n_passed native_d14n_failed native_d14n_status native_d14n_tests
  local wasm_default_passed wasm_default_failed wasm_default_status wasm_default_tests
  local wasm_d14n_passed wasm_d14n_failed wasm_d14n_status wasm_d14n_tests

  IFS='|' read -r native_default_passed native_default_failed native_default_status native_default_tests <<< "$native_default"
  IFS='|' read -r native_d14n_passed native_d14n_failed native_d14n_status native_d14n_tests <<< "$native_d14n"
  IFS='|' read -r wasm_default_passed wasm_default_failed wasm_default_status wasm_default_tests <<< "$wasm_default"
  IFS='|' read -r wasm_d14n_passed wasm_d14n_failed wasm_d14n_status wasm_d14n_tests <<< "$wasm_d14n"

  # Calculate totals
  local native_default_total=$((native_default_passed + native_default_failed))
  local native_d14n_total=$((native_d14n_passed + native_d14n_failed))
  local wasm_default_total=$((wasm_default_passed + wasm_default_failed))
  local wasm_d14n_total=$((wasm_d14n_passed + wasm_d14n_failed))
  local total_failures=$((native_default_failed + native_d14n_failed + wasm_default_failed + wasm_d14n_failed))

  # Format status displays
  local native_default_display native_d14n_display wasm_default_display wasm_d14n_display
  native_default_display=$(format_status "$native_default_passed" "$native_default_failed" "$native_default_status")
  native_d14n_display=$(format_status "$native_d14n_passed" "$native_d14n_failed" "$native_d14n_status")
  wasm_default_display=$(format_status "$wasm_default_passed" "$wasm_default_failed" "$wasm_default_status")
  wasm_d14n_display=$(format_status "$wasm_d14n_passed" "$wasm_d14n_failed" "$wasm_d14n_status")

  # Generate issue body
  generate_issue_body > issue-body.md

  # Find existing tracking issue
  local issue_number
  issue_number=$(gh issue list --label "ignored-tests-tracker" --state open --json number --jq '.[0].number // empty')

  if [[ -z "$issue_number" ]]; then
    gh issue create \
      --title "Ignored Tests Status Report" \
      --body-file issue-body.md \
      --label "ignored-tests-tracker"
    echo "Created new tracking issue"
  else
    # Get previous failure count from issue body (using awk for portability)
    local previous_failures
    previous_failures=$(gh issue view "$issue_number" --json body --jq '.body' | awk -F'/' '/\/ [0-9]+ :x:/ {gsub(/[^0-9]/, "", $2); if ($2 != "") print $2}' | paste -sd+ | bc 2>/dev/null || echo "0")

    gh issue edit "$issue_number" --body-file issue-body.md
    echo "Updated tracking issue #${issue_number}"

    # Comment if failures increased
    if [[ $total_failures -gt ${previous_failures:-0} ]]; then
      local increase=$((total_failures - previous_failures))
      gh issue comment "$issue_number" --body ":warning: **Failure count increased by ${increase}** (from ${previous_failures:-0} to ${total_failures}) in [run #${RUN_NUMBER}](${RUN_URL})"
      echo "::warning::Failure count increased from ${previous_failures:-0} to ${total_failures}"
    fi
  fi
}

main "$@"
