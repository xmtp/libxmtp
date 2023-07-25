#!/bin/bash
set -ex
RUN_ID=$(gh api /repos/$GITHUB_OWNER/$GITHUB_REPO/actions/runs \
    -X GET \
    -F head_sha=$HEAD_SHA \
    --jq '.workflow_runs[] | select(.name == "Build") | .id')
ARTIFACT_ID=$(gh api /repos/$GITHUB_OWNER/$GITHUB_REPO/actions/runs/$RUN_ID/artifacts \
    --jq '.artifacts[] | select(.name == "libxmtp-android.zip") | .id')
gh api /repos/$GITHUB_OWNER/$GITHUB_REPO/actions/artifacts/$ARTIFACT_ID/zip >artifact.zip
unzip -o artifact.zip
