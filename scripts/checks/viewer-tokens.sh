#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# A viewer screen names a role and a step from the token layer, never a palette
# entry, a raw type size, or a per-element theme prefix. The palette is measured
# once, in `e2e/tests/it/accessibility.rs`, and a colour written into a screen
# escapes that measurement silently. This is the check that fails instead.
#
#   scripts/checks/viewer-tokens.sh
#
# The roles and the steps are ferrohealth-ui's (its stylesheet, served by the
# console), the brand is `app/ferroterm-viewer/style/theme.css`, the utilities
# that name them `app/ferroterm-viewer/style/tailwind.css`, and the class
# strings a screen paints with `app/ferroterm-viewer/src/styles.rs`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly SOURCE_DIR="app/ferroterm-viewer/src"

if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "viewer-tokens: no $SOURCE_DIR — skipped."
  exit 0
fi

# Each rule is a name, an extended regular expression, and what to write
# instead. No file under src/ is exempt: `styles.rs` holds the class strings
# and it, too, names roles and steps rather than the palette they resolve to.
failures=0

report() {
  local what=$1 instead=$2 hits=$3
  if [[ -n "$hits" ]]; then
    failures=$((failures + 1))
    echo "viewer-tokens: $what"
    echo "  write $instead"
    printf '%s\n' "$hits" | sed 's/^/  /'
  fi
}

palette='(slate|gray|zinc|neutral|stone|teal|cyan|emerald|amber|red|sky|indigo)-[0-9]{2,3}'
report "a screen names a palette entry" \
  "a colour role: text-ink, text-muted, bg-surface, bg-surface-raised, border-edge, text-accent" \
  "$(grep -rnE "\b$palette\b" --include='*.rs' "$SOURCE_DIR" || true)"

report "a screen carries a per-element dark variant" \
  "one role, redefined once per theme by ferrohealth-ui and style/theme.css" \
  "$(grep -rnE '\bdark:[a-z-]' --include='*.rs' "$SOURCE_DIR" || true)"

report "a screen names a raw type size" \
  "a step: text-display, text-title, text-heading, text-body, text-small, text-micro" \
  "$(grep -rnE '\btext-(xs|sm|base|lg|xl|[2-9]xl)\b' --include='*.rs' "$SOURCE_DIR" || true)"

# `break-words` was renamed in Tailwind 4 and still parses, so it fails quietly
# rather than loudly (https://tailwindcss.com/docs/upgrade-guide).
report "a screen uses a class Tailwind 4 renamed" \
  "wrap-break-word" \
  "$(grep -rnE '\bbreak-words\b' --include='*.rs' "$SOURCE_DIR" || true)"

# Margin, padding and gap come from the four steps and nothing between them, so
# the rhythm of one screen is the rhythm of every screen. Sizes (h-4, w-56,
# max-w-md) are not spacing and are left alone.
spacing='(m|p|mt|mr|mb|ml|mx|my|pt|pr|pb|pl|px|py|gap|gap-x|gap-y|space-x|space-y)'
report "a screen names a spacing value off the scale" \
  "a step: tight, default, loose, section" \
  "$(grep -rnE "\b$spacing-([0-9]|\[)" --include='*.rs' "$SOURCE_DIR" || true)"

if [[ $failures -gt 0 ]]; then
  echo "viewer-tokens: $failures rule(s) broken."
  exit 1
fi

echo "viewer-tokens: every screen names roles and steps only."
