#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every first-party file in a format that takes a comment opens with both SPDX
# lines: the copyright holder and the licence (SPDX file tags,
# <https://spdx.github.io/spdx-spec/v2.3/file-tags/>; which files carry them
# is our own design). The formats are Rust, shell, TOML, YAML, Markdown, HTML,
# CSS and the Dockerfiles. JSON and other comment-less data carry none.
#
# Skipped: every directory that holds a PROVENANCE.md (vendored trees keep
# their upstream terms) and the changelog fragments under changelog.d/, whose
# text moves into CHANGELOG.md.
#
# The two lines sit in the head of the file: within its first eight lines,
# after a Markdown file's front matter, which must stay on line 1 for the
# tools that read it. A shebang, a doctype or a Dockerfile parser directive
# may come first.
#
#   scripts/checks/spdx-headers.sh              check the tree
#   scripts/checks/spdx-headers.sh --self-test  check the check
#
# Exit 0 when every file carries both lines. Exit 1 naming each file that does
# not.
set -euo pipefail

if [[ "${1:-}" == "--self-test" ]]; then
  script="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  mkdir -p "$tmp/scripts/checks" "$tmp/vendor/x" "$tmp/changelog.d" "$tmp/docs"
  cp "$script" "$tmp/scripts/checks/spdx-headers.sh"
  h1='SPDX-FileCopyrightText: Cadasto B.V.'
  h2='SPDX-License-Identifier: BUSL-1.1'
  printf '// %s\n// %s\n\nfn main() {}\n' "$h1" "$h2" >"$tmp/good.rs"
  printf '#!/usr/bin/env bash\n# %s\n# %s\n' "$h1" "$h2" >"$tmp/good.sh"
  printf -- '---\npaths: ["x"]\nmore: y\n---\n\n<!-- %s -->\n<!-- %s -->\n\n# T\n' "$h1" "$h2" >"$tmp/docs/front.md"
  printf '<!doctype html>\n<!-- %s -->\n<!-- %s -->\n' "$h1" "$h2" >"$tmp/docs/page.html"
  printf '# syntax=docker/dockerfile:1\n# %s\n# %s\nFROM scratch\n' "$h1" "$h2" >"$tmp/Dockerfile"
  printf 'no header\n' >"$tmp/vendor/x/kept.md"
  printf 'upstream\n' >"$tmp/vendor/PROVENANCE.md"
  printf -- '- An entry.\n' >"$tmp/changelog.d/1-entry.added.md"
  printf '{}\n' >"$tmp/data.json"
  (
    cd "$tmp"
    git init -q
    git add -A
    bash scripts/checks/spdx-headers.sh >/dev/null
  ) || { echo "self-test: a tree with every header failed" >&2; exit 1; }
  printf '// %s\nfn main() {}\n' "$h1" >"$tmp/half.rs"
  printf 'x = 1\n' >"$tmp/bare.toml"
  printf -- '---\na: b\n---\n\n\n\n\n\n\n\n\n<!-- %s -->\n<!-- %s -->\n' "$h1" "$h2" >"$tmp/docs/late.md"
  printf '# T\n\nSPDX-FileCopyrightText: Cadasto B.V. in prose\n%s in prose\n' "$h2" >"$tmp/docs/prose.md"
  out="$(cd "$tmp" && git add -A && bash scripts/checks/spdx-headers.sh 2>&1)" && {
    echo "self-test: a tree with missing headers passed" >&2
    exit 1
  }
  for want in half.rs bare.toml docs/late.md docs/prose.md; do
    grep -q "^spdx-headers: $want " <<<"$out" || {
      echo "self-test: $want was not reported" >&2
      exit 1
    }
  done
  if grep -qE "good\.|front\.md|page\.html|Dockerfile|kept\.md|entry\.added|data\.json" <<<"$out"; then
    echo "self-test: a file with its header, or out of scope, was reported" >&2
    exit 1
  fi
  echo "ok: spdx-headers self-test (missing, half and late headers fail; vendored, fragments and JSON are skipped)"
  exit 0
fi

if [[ $# -ne 0 ]]; then
  echo "usage: $0 [--self-test]" >&2
  exit 2
fi

cd "$(dirname "$0")/../.."

excludes=(':(exclude)changelog.d/[0-9]*.md')
while IFS= read -r provenance; do
  excludes+=(":(exclude)$(dirname "$provenance")/**")
done < <(git ls-files '*PROVENANCE.md')

missing=0
while IFS= read -r file; do
  [[ -f "$file" ]] || continue
  if ! awk '
    NR == 1 && FILENAME ~ /\.md$/ && $0 == "---" { front = 1; next }
    front { if ($0 == "---") front = 0; next }
    { n++ }
    n > 8 { exit }
    /^[[:space:]]*(\/\/|#|<!--|\/\*|\*)[[:space:]]*SPDX-FileCopyrightText:/ { holder = 1 }
    /^[[:space:]]*(\/\/|#|<!--|\/\*|\*)[[:space:]]*SPDX-License-Identifier:/ { licence = 1 }
    END { exit !(holder && licence) }
  ' "$file"; then
    echo "spdx-headers: $file lacks the two SPDX lines in its head" >&2
    missing=$((missing + 1))
  fi
done < <(git ls-files -- '*.rs' '*.sh' '*.toml' '*.yml' '*.yaml' '*.md' \
  '*.html' '*.css' '*Dockerfile' '*Dockerfile.*' "${excludes[@]}")

if [[ $missing -ne 0 ]]; then
  echo "spdx-headers: $missing file(s) without the header; add" >&2
  echo "  SPDX-FileCopyrightText: Cadasto B.V." >&2
  echo "  SPDX-License-Identifier: BUSL-1.1 (Apache-2.0 under crates/rf2)" >&2
  echo "as comments in the file's own syntax" >&2
  exit 1
fi
echo "spdx-headers: every first-party file carries both SPDX lines"
