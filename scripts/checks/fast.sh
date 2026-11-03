#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The default local gate: rustfmt, clippy, nextest and the doctests on the
# workspace packages a change reaches. CI stays the full gate: its Rust lanes
# run every package on every pull request (.github/workflows/ci.yml).
# No specification governs this: our own design (docs/ci-cd.md).
#
# A package is reached when the change touches a file under its directory,
# or when it depends, directly or through other workspace packages, on one
# that is. A change to any other file reaches every package, because the
# packages read the root manifests, the toolchain and lint files, and files
# under data/, website/ and the root CHANGELOG.md, except for the files no
# build reads: the ones under .claude/, .github/, changelog.d/, docs/,
# scripts/ and security/, and the root's other Markdown.
#
# Usage:
#   scripts/checks/fast.sh [--base <rev>] [--all] [--dry-run]
#
#   --base <rev>   measure from this revision instead of the merge base
#                  with origin/main
#   --all          every package, as CI runs them
#   --dry-run      print the plan and run nothing
#
# The change is measured from the base to the working tree, untracked files
# included. Each step is one cargo invocation over the whole reached set, so
# a shared dependency builds once.
# Exit 0 when every step passed, 1 when one failed, 2 on a usage error.
set -euo pipefail
cd "$(dirname "$0")/../.."

usage() {
  sed -n '/^# Usage:/,/^# The change is/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//' >&2
  exit 2
}

base=""
all=false
dry=false
while [ $# -gt 0 ]; do
  case "$1" in
    --base)
      [ $# -ge 2 ] || usage
      base="$2"
      shift 2
      ;;
    --all)
      all=true
      shift
      ;;
    --dry-run)
      dry=true
      shift
      ;;
    *) usage ;;
  esac
done

root="$(pwd -P)"
metadata="$(cargo metadata --format-version 1 --locked)"

# Every workspace package as `name<TAB>directory`, the directory relative to
# the root.
packages_and_dirs="$(jq -r --arg root "$root/" '
  .workspace_members as $members
  | .packages[]
  | select(.id as $id | $members | index($id))
  | [.name, (.manifest_path | ltrimstr($root) | sub("/Cargo.toml$"; ""))]
  | @tsv
' <<<"$metadata")"

if [ "$all" = false ]; then
  if [ -z "$base" ]; then
    base="$(git merge-base origin/main HEAD)" || {
      echo "fast: no merge base with origin/main; fetch it or pass --base <rev>" >&2
      exit 1
    }
  fi
  changed="$({
    git diff --name-only "$base" --
    git ls-files --others --exclude-standard
  } | sort -u)"
  touched=""
  while IFS= read -r file; do
    [ -n "$file" ] || continue
    owner="$(awk -F'\t' -v file="$file" '
      index(file, $2 "/") == 1 && length($2) > best { best = length($2); name = $1 }
      END { print name }
    ' <<<"$packages_and_dirs")"
    if [ -n "$owner" ]; then
      touched="$touched $owner"
      continue
    fi
    case "$file" in
      .claude/* | .github/* | changelog.d/* | docs/* | scripts/* | security/*) ;;
      CHANGELOG.md) all=true ;;
      */*) all=true ;;
      *.md) ;;
      *) all=true ;;
    esac
  done <<<"$changed"
fi

if [ "$all" = true ]; then
  reached="$(cut -f1 <<<"$packages_and_dirs" | sort -u | tr '\n' ' ')"
else
  # The touched packages and every workspace package that depends on one, to
  # a fixed point over the resolved dependency graph.
  reached="$(jq -r --arg touched "$touched" '
    .workspace_members as $members
    | ([.packages[] | {key: .id, value: .name}] | from_entries) as $names
    | [.resolve.nodes[] | select(.id as $id | $members | index($id))
       | {name: $names[.id], deps: [.dependencies[] | $names[.]]}] as $nodes
    | ($touched | split(" ") | map(select(. != ""))) as $seed
    | def grow($set):
        ($set + [$nodes[] | select(any(.deps[]; . as $d | $set | index($d))) | .name]
         | unique) as $next
        | if ($next | length) == ($set | length) then $set else grow($next) end;
      grow($seed | unique) | join(" ")
  ' <<<"$metadata")"
fi

if [ -z "${reached// /}" ]; then
  echo "fast: the change reaches no package; nothing to run"
  exit 0
fi
echo "fast: reached: $reached"

run() {
  echo "+ $*"
  if [ "$dry" = false ]; then
    "$@"
  fi
}

args=()
for package in $reached; do args+=(-p "$package"); done
run cargo fmt --check "${args[@]}"
run cargo clippy --locked "${args[@]}" --all-targets --all-features -- -D warnings
run cargo nextest run --locked "${args[@]}" --no-tests=pass
# `cargo test --doc` refuses a package without a library target.
libraries=" $(jq -r '
  .workspace_members as $members
  | .packages[]
  | select(.id as $id | $members | index($id))
  | select(any(.targets[]; any(.kind[]; . == "lib" or . == "rlib")))
  | .name
' <<<"$metadata" | tr '\n' ' ')"
docs=()
for package in $reached; do
  case "$libraries" in *" $package "*) docs+=(-p "$package") ;; esac
done
if [ "${#docs[@]}" -gt 0 ]; then
  run cargo test --doc --locked "${docs[@]}"
fi
if [ "$dry" = true ]; then
  echo "fast: dry run, nothing ran"
else
  echo "fast: every step passed"
fi
