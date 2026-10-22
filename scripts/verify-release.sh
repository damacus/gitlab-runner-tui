#!/usr/bin/env bash
# Read-only preflight: publication must use the event commit after its gates pass.
set -euo pipefail

: "${GITHUB_REPOSITORY:?}" "${GITHUB_SHA:?}" "${RELEASE_TAG:?}"
[[ "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]] || exit 1
[[ "$RELEASE_TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 1

tag_sha=$(gh api "repos/$GITHUB_REPOSITORY/commits/$RELEASE_TAG" --jq .sha)
if [[ "$tag_sha" != "$GITHUB_SHA" ]]; then
  echo '::error::Release tag does not identify the workflow event commit'
  exit 1
fi

for workflow in ci.yml security.yml; do
  passed=false
  for attempt in {1..60}; do
    result=$(gh api --method GET \
      "repos/$GITHUB_REPOSITORY/actions/workflows/$workflow/runs" \
      -f "head_sha=$GITHUB_SHA" -f event=push -f branch=main -f per_page=100 \
      --jq '.workflow_runs | max_by(.id) | if . == null then "pending" elif .status != "completed" then "pending" else .conclusion end')
    case "$result" in
      success) passed=true; break ;;
      pending) sleep 15 ;;
      *) echo "::error::$workflow did not pass ($result)"; exit 1 ;;
    esac
  done
  if [[ "$passed" != true ]]; then
    echo "::error::Timed out waiting for $workflow"
    exit 1
  fi
done
