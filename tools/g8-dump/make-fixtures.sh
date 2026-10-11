#!/usr/bin/env bash
#
# Creates the G8 fixture repositories under tests/fixtures/repos/ deterministically.
#
# Determinism is the point: every commit has a fixed author/committer name, email and
# date, and a fixed message, so the resulting object hashes are stable across machines
# and the golden JSON files stay valid. Default branch is pinned to `main`, gpg signing
# is off, autocrlf is off, and the system git config is ignored.
#
# The script is ADD-ONLY: an existing fixture repository is verified, never deleted or
# rewritten. To regenerate from scratch, remove the fixture directory yourself
# (see README.md) and re-run.
#
# Usage: bash make-fixtures.sh [output-dir]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../.." && pwd)"
# Optional override (tests/determinism checks): bash make-fixtures.sh <output-dir>
REPOS="${1:-${ROOT}/tests/fixtures/repos}"

export GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME="G8 Fixture"
export GIT_AUTHOR_EMAIL="g8@example.invalid"
export GIT_COMMITTER_NAME="G8 Fixture"
export GIT_COMMITTER_EMAIL="g8@example.invalid"

GIT=(git
  -c init.defaultBranch=main
  -c core.autocrlf=false
  -c commit.gpgsign=false
  -c tag.gpgsign=false
  -c advice.detachedHead=false)

# commit <repo> <iso-date> <message> <file> <content>
commit() {
  local repo="$1" date="$2" message="$3" file="$4" content="$5"
  printf '%s\n' "${content}" >"${repo}/${file}"
  GIT_AUTHOR_DATE="${date}" GIT_COMMITTER_DATE="${date}" \
    "${GIT[@]}" -C "${repo}" add -A
  GIT_AUTHOR_DATE="${date}" GIT_COMMITTER_DATE="${date}" \
    "${GIT[@]}" -C "${repo}" commit -q -m "${message}"
}

# new_repo <name> — creates the repository only when it does not exist yet.
new_repo() {
  local name="$1"
  local dir="${REPOS}/${name}"
  if [ -e "${dir}" ]; then
    echo "keep  ${name} (already exists, left untouched)"
    return 1
  fi
  mkdir -p "${dir}"
  "${GIT[@]}" -C "${dir}" init -q
  echo "make  ${name}"
  return 0
}

mkdir -p "${REPOS}"

# ---------------------------------------------------------------- fixture: linear
# Five commits on one branch, no merges: the simplest possible lane assignment.
if new_repo linear; then
  repo="${REPOS}/linear"
  commit "${repo}" "2024-01-01T00:00:00+00:00" "c1 initial"  file.txt "1"
  commit "${repo}" "2024-01-01T00:01:00+00:00" "c2 second"   file.txt "2"
  commit "${repo}" "2024-01-01T00:02:00+00:00" "c3 third"    file.txt "3"
  commit "${repo}" "2024-01-01T00:03:00+00:00" "c4 fourth"   file.txt "4"
  commit "${repo}" "2024-01-01T00:04:00+00:00" "c5 fifth"    file.txt "5"
  GIT_COMMITTER_DATE="2024-01-01T00:04:00+00:00" \
    "${GIT[@]}" -C "${repo}" tag -a v1 -m "release v1"
  echo "      tip: $("${GIT[@]}" -C "${repo}" rev-parse HEAD)"
fi

# ------------------------------------------------- fixture: branch-merge-rebase
# A merge plus a fork off an older base (the "rebase-like" case: two branches that
# share an ancestor but whose tips are not merged).
if new_repo branch-merge-rebase; then
  repo="${REPOS}/branch-merge-rebase"
  commit "${repo}" "2024-02-01T00:00:00+00:00" "m1 base"    a.txt "1"
  c2="$("${GIT[@]}" -C "${repo}" rev-parse HEAD)"
  commit "${repo}" "2024-02-01T00:01:00+00:00" "m2 second"  a.txt "2"
  commit "${repo}" "2024-02-01T00:02:00+00:00" "m3 third"   a.txt "3"
  base_main="$("${GIT[@]}" -C "${repo}" rev-parse HEAD)"

  # feature branch forks at m1 (rebase-like: not at the tip).
  "${GIT[@]}" -C "${repo}" switch -q -c feature "${c2}"
  commit "${repo}" "2024-02-01T00:03:00+00:00" "f1 feature work" b.txt "1"
  commit "${repo}" "2024-02-01T00:04:00+00:00" "f2 feature done" b.txt "2"

  # main advances, then merges feature (--no-ff keeps the merge commit explicit).
  "${GIT[@]}" -C "${repo}" switch -q main
  commit "${repo}" "2024-02-01T00:05:00+00:00" "m4 on main"  a.txt "4"
  GIT_AUTHOR_DATE="2024-02-01T00:06:00+00:00" GIT_COMMITTER_DATE="2024-02-01T00:06:00+00:00" \
    "${GIT[@]}" -C "${repo}" merge -q --no-ff feature -m "merge feature into main"

  # second fork off the oldest commit, left unmerged.
  "${GIT[@]}" -C "${repo}" switch -q -c rebase-like "${c2}"
  commit "${repo}" "2024-02-01T00:07:00+00:00" "r1 replanted" c.txt "1"
  commit "${repo}" "2024-02-01T00:08:00+00:00" "r2 replanted again" c.txt "2"

  "${GIT[@]}" -C "${repo}" switch -q main
  GIT_COMMITTER_DATE="2024-02-01T00:09:00+00:00" \
    "${GIT[@]}" -C "${repo}" tag -a v2 -m "release v2" main
  echo "      main tip: ${base_main} -> $("${GIT[@]}" -C "${repo}" rev-parse main)"
  echo "      branches: $("${GIT[@]}" -C "${repo}" for-each-ref --format='%(refname:short)' refs/heads | tr '\n' ' ')"
fi

echo
echo "fixture repositories under ${REPOS}:"
ls -1 "${REPOS}"
