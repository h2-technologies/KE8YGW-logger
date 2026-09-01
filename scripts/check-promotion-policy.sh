#!/usr/bin/env bash
# Enforce the branch promotion policy documented in
# docs/BRANCHING_AND_RELEASE_CHANNELS.md.
#
# Pull requests into `main` must come from `dev` in this repository. Emergency
# `hotfix/*` branches are the one exception, and only when the pull request body
# documents the immediate synchronization back into `dev`. Pull requests into
# any other branch are outside the policy and always pass.
#
# Both the `Main promotion policy` job in .github/workflows/ci.yml and the
# `promotion-policy` job in .github/workflows/branch-promotion-policy.yml call
# this script so the two checks cannot drift apart.
#
# Callers MUST check out the pull request's base commit rather than the merge
# ref. This script is the promotion gate, so running the head branch's copy of
# it would let any pull request author edit the gate that judges them.
#
# Required environment:
#   BASE_BRANCH      base branch of the pull request
#   HEAD_BRANCH      head branch of the pull request
#   BASE_REPOSITORY  owner/name of the repository the pull request targets
#   HEAD_REPOSITORY  owner/name of the repository the head branch lives in
#   PR_BODY          pull request description
set -euo pipefail

base_branch="${BASE_BRANCH:-}"
head_branch="${HEAD_BRANCH:-}"
base_repository="${BASE_REPOSITORY:-}"
head_repository="${HEAD_REPOSITORY:-}"
pr_body="${PR_BODY:-}"

if [[ "$base_branch" != "main" ]]; then
  echo "Pull request into ${base_branch:-<unknown>} is allowed by the promotion policy."
  exit 0
fi

if [[ "$head_repository" != "$base_repository" ]]; then
  echo "::error title=Invalid promotion::Pull requests into main must originate from ${base_repository}, not ${head_repository:-<unknown>}."
  exit 1
fi

if [[ "$head_branch" == "dev" ]]; then
  echo "Valid promotion: ${base_repository}:dev -> ${base_repository}:main"
  exit 0
fi

if [[ "$head_branch" == hotfix/* ]]; then
  if grep -Eiq 'sync(ing)? (back )?(to|into) dev|follow-up.*dev|backport.*dev' <<<"$pr_body"; then
    echo "Documented hotfix exception accepted for ${head_branch}."
    exit 0
  fi
  echo "::error title=Undocumented hotfix::Hotfix pull requests into main must document the immediate follow-up synchronization back into dev." >&2
  exit 1
fi

echo "::error title=Invalid promotion::Normal pull requests into main must originate from dev. Use hotfix/* only for documented emergency fixes." >&2
exit 1
