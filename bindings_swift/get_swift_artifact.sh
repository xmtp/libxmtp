#!/bin/bash
set -ex

# Figure out the id of the Build run for the specified $HEAD_SHA
RUN_ID=$(gh api /repos/$GITHUB_REPO/actions/runs \
    -X GET \
    -F head_sha=$HEAD_SHA \
    --jq '.workflow_runs[] | select(.name == "Build") | .id')
# Get the id of the xmtp-rust-swift.zip artifacts for the run $RUN_ID
ARTIFACT_ID=$(gh api /repos/$GITHUB_REPO/actions/runs/$RUN_ID/artifacts \
    --jq '.artifacts[] | select(.name == "xmtp-rust-swift.zip") | .id')
# Fetch artifact $ARTIFACT_ID and unzip it because it will be double compressed
# artifact.zip -> xmtp-rust-swift.zip
gh api /repos/$GITHUB_REPO/actions/artifacts/$ARTIFACT_ID/zip >artifact.zip
unzip -o artifact.zip
