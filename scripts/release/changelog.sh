#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
#
# Changelog fragments (no specification governs this: our own design). Every
# change with a user-visible effect adds one file under changelog.d/, so no two
# pull requests edit the same lines of CHANGELOG.md, and the release cut
# assembles the fragments into CHANGELOG.md, which follows Keep a Changelog
# 1.1.0 (https://keepachangelog.com/en/1.1.0/).
#
# A fragment is changelog.d/<issue>-<kebab-slug>.<section>.md, where <section>
# is added, changed, deprecated, removed, fixed or security, and its content is
# one or more Markdown list items exactly as they read in CHANGELOG.md: a line
# that opens with "- ", continued by lines indented by at least two spaces.
# changelog.d/README.md is the format note and is never read as a fragment.
#
#   changelog.sh --check
#       Validates every fragment's name and content. Exit 0 when every one
#       holds, 1 otherwise, naming each defect.
#   changelog.sh --assemble <version> <date>
#       Writes a new "## [<version>] - <date>" section under a fresh, empty
#       [Unreleased], holding the entries still under [Unreleased] followed by
#       the fragments, section by section in the Keep a Changelog order; moves
#       the [Unreleased] link reference on and adds the version's; then
#       `git rm`s the fragments. Exit 1 on any defect, with nothing written.
#   changelog.sh --self-test
#       Proves both modes over a stub repository.
#
# The output is deterministic: fragments are read by section in the order
# above, then by file name in the C locale. Exit 2 on a usage error.
set -euo pipefail
export LC_ALL=C

# The Keep a Changelog 1.1.0 sections, in the order a release lists them.
readonly SECTIONS=(added changed deprecated removed fixed security)
readonly FRAGMENT_DIR=changelog.d
readonly NAME_RE='^[0-9]+-[a-z0-9]+(-[a-z0-9]+)*\.(added|changed|deprecated|removed|fixed|security)\.md$'

die() {
  echo "changelog: $*" >&2
  exit 1
}

usage() {
  echo "usage: changelog.sh --check | --assemble <version> <date> | --self-test" >&2
  exit 2
}

# title SECTION: the heading a section carries in CHANGELOG.md.
title() {
  local section="$1"
  printf '%s%s' "$(tr '[:lower:]' '[:upper:]' <<<"${section:0:1}")" "${section:1}"
}

# content_defect FILE: prints why FILE is no list item, or nothing when it is.
content_defect() {
  local file="$1"
  awk '
    { lines[NR] = $0 }
    END {
      last = NR
      while (last > 0 && lines[last] ~ /^[[:space:]]*$/) last--
      if (last == 0) { print "it is empty"; exit }
      if (lines[1] !~ /^- [^[:space:]]/) { print "line 1 does not open a list item with \"- \""; exit }
      for (i = 2; i <= last; i++) {
        if (lines[i] ~ /^- [^[:space:]]/ || lines[i] ~ /^  +[^[:space:]]/) continue
        if (lines[i] ~ /^[[:space:]]*$/) { print "line " i " is blank inside the entry"; exit }
        print "line " i " neither opens a list item nor continues one indented by two spaces"
        exit
      }
    }
  ' "$file"
}

# check: every fragment's name and content, every defect named.
check() {
  local fail=0 count=0 path name defect
  [[ -d "$FRAGMENT_DIR" ]] || die "$FRAGMENT_DIR/ is missing"
  for path in "$FRAGMENT_DIR"/* "$FRAGMENT_DIR"/.[!.]*; do
    [[ -e "$path" ]] || continue
    name="${path#"$FRAGMENT_DIR"/}"
    [[ "$name" != README.md ]] || continue
    if [[ ! -f "$path" ]]; then
      echo "changelog: $path is not a file; a fragment is one file directly under $FRAGMENT_DIR/." >&2
      fail=1
      continue
    fi
    if [[ ! "$name" =~ $NAME_RE ]]; then
      echo "changelog: $path is not named <issue>-<kebab-slug>.<section>.md with <section> one of ${SECTIONS[*]}." >&2
      fail=1
      continue
    fi
    defect="$(content_defect "$path")"
    if [[ -n "$defect" ]]; then
      echo "changelog: $path is not a Markdown list item: $defect." >&2
      fail=1
      continue
    fi
    count=$((count + 1))
  done
  [[ "$fail" -eq 0 ]] || exit 1
  echo "changelog: $count fragment(s) under $FRAGMENT_DIR/, every one well formed."
}

# assemble VERSION DATE: the cut, written to CHANGELOG.md in one move.
assemble() {
  local version="$1" date="$2" work start end total section file prev base out
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || die "'$version' is not a version such as 0.0.9 or 0.0.9-rc.1"
  [[ "$date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || die "'$date' is not a date written YYYY-MM-DD"
  [[ -f CHANGELOG.md ]] || die "CHANGELOG.md is missing"
  check > /dev/null

  if awk -v heading="## [$version]" 'index($0, heading) == 1 { found = 1 } END { exit !found }' CHANGELOG.md; then
    die "CHANGELOG.md already has a [$version] section"
  fi
  start="$(grep -n -m1 -E '^## \[Unreleased\][[:space:]]*$' CHANGELOG.md | cut -d: -f1 || true)"
  [[ -n "$start" ]] || die "CHANGELOG.md has no '## [Unreleased]' heading"
  total="$(wc -l < CHANGELOG.md | tr -d ' ')"
  # The [Unreleased] body ends at the next version heading or at the link
  # reference block, whichever comes first.
  end="$(awk -v s="$start" 'NR > s && (/^## \[/ || /^\[[^]]+\]: /) { print NR - 1; exit }' CHANGELOG.md)"
  [[ -n "$end" ]] || end="$total"

  prev="$(sed -nE 's|^\[Unreleased\]: (.*)/compare/([^/]+)\.\.\.HEAD$|\2|p' CHANGELOG.md)"
  base="$(sed -nE 's|^\[Unreleased\]: (.*)/compare/([^/]+)\.\.\.HEAD$|\1|p' CHANGELOG.md)"
  [[ -n "$prev" && -n "$base" ]] || die "CHANGELOG.md has no '[Unreleased]: <repository>/compare/<tag>...HEAD' link reference"

  work="$(mktemp -d)"
  # Global, so the EXIT trap still sees it after the function returns.
  assemble_work="$work"
  trap 'rm -r -- "$assemble_work"' EXIT

  # The entries still under [Unreleased], one file per section. Anything
  # outside a known section heading is refused, never dropped.
  sed -n "$((start + 1)),${end}p" CHANGELOG.md | awk -v dir="$work" '
    /^### / {
      heading = substr($0, 5)
      sub(/[[:space:]]+$/, "", heading)
      section = tolower(heading)
      if (section !~ /^(added|changed|deprecated|removed|fixed|security)$/) {
        print "changelog: [Unreleased] has a section \"" heading "\", which Keep a Changelog 1.1.0 does not define." > "/dev/stderr"
        exit 1
      }
      next
    }
    section == "" && /[^[:space:]]/ {
      print "changelog: [Unreleased] holds text outside a section heading: " $0 > "/dev/stderr"
      exit 1
    }
    section != "" { print > (dir "/old." section) }
  ' || exit 1

  # Each section of the release: the [Unreleased] entries first, in their
  # order, then the fragments by file name.
  local fragments=() entries=0
  for section in "${SECTIONS[@]}"; do
    {
      [[ ! -f "$work/old.$section" ]] || trim "$work/old.$section"
      for file in "$FRAGMENT_DIR"/*."$section".md; do
        [[ -e "$file" ]] || continue
        git ls-files --error-unmatch -- "$file" > /dev/null 2>&1 || die "$file is not tracked by git; commit it before the cut"
        fragments+=("$file")
        trim "$file"
      done
    } > "$work/new.$section"
    [[ ! -s "$work/new.$section" ]] || entries=$((entries + 1))
  done
  [[ "$entries" -gt 0 ]] || die "[$version] would be empty: no fragment under $FRAGMENT_DIR/ and no entry under [Unreleased]"

  out="$work/CHANGELOG.md"
  {
    sed -n "1,${start}p" CHANGELOG.md
    printf '\n## [%s] - %s\n' "$version" "$date"
    for section in "${SECTIONS[@]}"; do
      [[ -s "$work/new.$section" ]] || continue
      printf '\n### %s\n\n' "$(title "$section")"
      cat "$work/new.$section"
    done
    printf '\n'
    if [[ "$end" -lt "$total" ]]; then
      sed -n "$((end + 1)),${total}p" CHANGELOG.md | awk -v ver="$version" -v prev="$prev" -v base="$base" '
        /^\[Unreleased\]: / {
          print "[Unreleased]: " base "/compare/v" ver "...HEAD"
          print "[" ver "]: " base "/compare/" prev "...v" ver
          next
        }
        { print }
      '
    fi
  } > "$out"

  cat "$out" > CHANGELOG.md
  if [[ "${#fragments[@]}" -gt 0 ]]; then
    git rm -q -- "${fragments[@]}"
  fi
  echo "changelog: [$version] - $date assembled from ${#fragments[@]} fragment(s) and the [Unreleased] entries; review CHANGELOG.md and commit."
}

# trim FILE: FILE without its leading and trailing blank lines.
trim() {
  local file="$1"
  awk '
    { lines[NR] = $0 }
    END {
      first = 1
      while (first <= NR && lines[first] ~ /^[[:space:]]*$/) first++
      last = NR
      while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
      for (i = first; i <= last; i++) print lines[i]
    }
  ' "$file"
}

self_test() {
  local script status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  # git writes its objects read-only, so the tree is made writable before it
  # is removed.
  trap 'chmod -R u+w "$work" && rm -r -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  mkdir -p "$work/scripts/release" "$work/$FRAGMENT_DIR"
  cp "$script" "$work/scripts/release/changelog.sh"

  # A repository of its own, so no global hook, signing key or identity of the
  # caller's reaches it.
  stub_git() {
    git -C "$work" -c user.name=self-test -c user.email=self-test@example.org \
      -c commit.gpgsign=false -c core.hooksPath=/dev/null "$@"
  }
  flunk() {
    echo "changelog: self-test failed: $*" >&2
    cat "$work/out" >&2
    exit 1
  }
  # expect NAME WANT ARGS...: the script run in the stub exits WANT.
  expect() {
    local name="$1" want="$2"
    shift 2
    status=0
    (cd "$work" && bash scripts/release/changelog.sh "$@") > "$work/out" 2>&1 || status=$?
    [[ "$status" -eq "$want" ]] || flunk "$name exited $status, wanted $want."
  }
  fragment() {
    local name="$1"
    shift
    printf '%s\n' "$@" > "$work/$FRAGMENT_DIR/$name"
  }
  unfragment() {
    local name="$1"
    rm -- "$work/$FRAGMENT_DIR/$name"
  }

  stub_git init -q -b main
  printf 'The fragment format.\n' > "$work/$FRAGMENT_DIR/README.md"

  # --check: the names and contents it accepts, and each it refuses.
  expect "a directory holding only the README" 0 --check
  fragment 3-a-change.added.md '- An entry that runs' '  over two lines.' '  - and nests a list item.'
  expect "a well-formed fragment beside the README" 0 --check
  local bad
  for bad in 3-Upper.added.md a-no-issue.added.md 3-a.unknown.md 3-a.added.txt 3-a_b.fixed.md 3-.fixed.md; do
    fragment "$bad" '- An entry.'
    expect "the misnamed fragment $bad" 1 --check
    grep -q "$bad is not named" "$work/out" || flunk "the misnamed fragment $bad failed without naming it."
    unfragment "$bad"
  done
  fragment 4-heading.fixed.md '### Fixed' '' '- An entry.'
  expect "a fragment holding a heading" 1 --check
  unfragment 4-heading.fixed.md
  fragment 4-prose.fixed.md 'A sentence, no list item.'
  expect "a fragment of prose" 1 --check
  unfragment 4-prose.fixed.md
  fragment 4-blank.fixed.md '- One entry.' '' '- Another entry.'
  expect "a fragment with a blank line inside" 1 --check
  grep -q 'line 2 is blank' "$work/out" || flunk "the blank line was not named."
  unfragment 4-blank.fixed.md
  fragment 4-flush.fixed.md '- One entry' 'continued flush left.'
  expect "a fragment continued without indentation" 1 --check
  unfragment 4-flush.fixed.md
  printf '' > "$work/$FRAGMENT_DIR/4-empty.fixed.md"
  expect "an empty fragment" 1 --check
  unfragment 4-empty.fixed.md
  mkdir "$work/$FRAGMENT_DIR/5-dir.added.md"
  expect "a directory named as a fragment" 1 --check
  rmdir "$work/$FRAGMENT_DIR/5-dir.added.md"

  # --assemble: the [Unreleased] entries an in-flight change wrote, merged with
  # the fragments, section by section and by file name in the C locale.
  cat > "$work/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

### Added

- An entry written into CHANGELOG.md
  before the fragments.

### Fixed

- A fix written into CHANGELOG.md.

## [0.0.1] - 2026-10-01

### Added

- The first release.

[Unreleased]: https://example.org/r/compare/v0.0.1...HEAD
[0.0.1]: https://example.org/r/releases/tag/v0.0.1
EOF
  fragment 12-b.added.md '- Fragment 12-b.'
  fragment 7-y.changed.md '- Fragment 7-y.' '' ''
  fragment 5-x.security.md '- Fragment 5-x.'
  stub_git add -A
  stub_git commit -q -m "the stub"

  expect "a version that is no version" 1 --assemble 0.0 2026-10-10
  expect "a date that is no date" 1 --assemble 0.0.2 10-10-2026
  expect "a missing argument" 2 --assemble 0.0.2
  expect "an existing version" 1 --assemble 0.0.1 2026-10-10
  expect "the cut" 0 --assemble 0.0.2 2026-10-10

  cat > "$work/want" <<'EOF'
# Changelog

## [Unreleased]

## [0.0.2] - 2026-10-10

### Added

- An entry written into CHANGELOG.md
  before the fragments.
- Fragment 12-b.
- An entry that runs
  over two lines.
  - and nests a list item.

### Changed

- Fragment 7-y.

### Fixed

- A fix written into CHANGELOG.md.

### Security

- Fragment 5-x.

## [0.0.1] - 2026-10-01

### Added

- The first release.

[Unreleased]: https://example.org/r/compare/v0.0.2...HEAD
[0.0.2]: https://example.org/r/compare/v0.0.1...v0.0.2
[0.0.1]: https://example.org/r/releases/tag/v0.0.1
EOF
  diff -u "$work/want" "$work/CHANGELOG.md" > "$work/out" || flunk "the assembled CHANGELOG.md differs from the expected one."
  if [[ -n "$(stub_git ls-files -- "$FRAGMENT_DIR" | grep -v '/README.md$' || true)" ]]; then
    flunk "a fragment is still tracked after the cut."
  fi
  [[ -f "$work/$FRAGMENT_DIR/README.md" ]] || flunk "the cut removed the README."
  expect "a check after the cut" 0 --check

  # A second cut with nothing to release, and an [Unreleased] the cut cannot
  # read, are refused with CHANGELOG.md untouched.
  stub_git commit -q -a -m "the cut"
  expect "a release with no entry" 1 --assemble 0.0.3 2026-10-11
  grep -q 'would be empty' "$work/out" || flunk "the empty release was not named."
  printf '%s\n' '# Changelog' '' '## [Unreleased]' '' '### Improved' '' '- An entry.' '' \
    '[Unreleased]: https://example.org/r/compare/v0.0.2...HEAD' > "$work/CHANGELOG.md"
  cp "$work/CHANGELOG.md" "$work/before"
  expect "a section Keep a Changelog does not define" 1 --assemble 0.0.3 2026-10-11
  cmp -s "$work/before" "$work/CHANGELOG.md" || flunk "a refused cut wrote CHANGELOG.md."
  printf '%s\n' '# Changelog' '' '## [Unreleased]' '' 'Loose prose.' '' '### Added' '' '- An entry.' '' \
    '[Unreleased]: https://example.org/r/compare/v0.0.2...HEAD' > "$work/CHANGELOG.md"
  expect "text outside a section heading" 1 --assemble 0.0.3 2026-10-11
  printf '%s\n' '# Changelog' '' '## [Unreleased]' '' '### Added' '' '- An entry.' > "$work/CHANGELOG.md"
  expect "no [Unreleased] link reference" 1 --assemble 0.0.3 2026-10-11

  echo "changelog: self-test passed."
}

case "${1:-}" in
--self-test)
  [[ $# -eq 1 ]] || usage
  self_test
  ;;
--check)
  [[ $# -eq 1 ]] || usage
  cd "$(dirname "$0")/../.."
  check
  ;;
--assemble)
  [[ $# -eq 3 ]] || usage
  cd "$(dirname "$0")/../.."
  assemble "$2" "$3"
  ;;
*) usage ;;
esac
