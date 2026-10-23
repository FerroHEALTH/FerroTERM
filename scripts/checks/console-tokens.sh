#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# A console screen names a role and a step from the token layer, never a palette
# entry, a raw type size, or a per-element theme prefix. The palette is measured
# once, in `e2e/tests/it/accessibility.rs`, and a colour written into a screen
# escapes that measurement silently. This is the check that fails instead.
#
#   scripts/checks/console-tokens.sh
#
# The roles and the steps are ferrohealth-ui's (its stylesheet, served by the
# console), the brand is `ferrohealth_ui::style::Brand::Term`, and the class
# strings a screen paints with `app/ferroterm-console/src/styles.rs`.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly SOURCE_DIR="app/ferroterm-console/src"

if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "console-tokens: no $SOURCE_DIR — skipped."
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
    echo "console-tokens: $what"
    echo "  write $instead"
    printf '%s\n' "$hits" | sed 's/^/  /'
  fi
}

palette='(slate|gray|zinc|neutral|stone|teal|cyan|emerald|amber|red|sky|indigo)-[0-9]{2,3}'
report "a screen names a palette entry" \
  "a colour role: text-ink, text-muted, bg-surface, bg-surface-raised, border-edge, text-accent" \
  "$(grep -rnE "\b$palette\b" --include='*.rs' "$SOURCE_DIR" || true)"

report "a screen carries a per-element dark variant" \
  "one role, redefined once per theme by ferrohealth-ui" \
  "$(grep -rnE '\bdark:[a-z-]' --include='*.rs' "$SOURCE_DIR" || true)"

report "a screen names a raw type size" \
  "a step: text-display, text-title, text-heading, text-body, text-small, text-micro" \
  "$(grep -rnE '\btext-(xs|sm|base|lg|xl|[2-9]xl)\b' --include='*.rs' "$SOURCE_DIR" || true)"

# Margin, padding and gap come from the four steps and nothing between them, so
# the rhythm of one screen is the rhythm of every screen. Sizes (h-4, w-56,
# max-w-md) are not spacing and are left alone.
spacing='(m|p|mt|mr|mb|ml|mx|my|pt|pr|pb|pl|px|py|gap|gap-x|gap-y|space-x|space-y)'
report "a screen names a spacing value off the scale" \
  "a step: tight, default, loose, section" \
  "$(grep -rnE "\b$spacing-([0-9]|\[)" --include='*.rs' "$SOURCE_DIR" || true)"

# Layout is FerroUI's: every class a screen writes in a literal names a
# ferrohealth-ui class, so the console carries no layout utility of its own.
utilities="$(grep -rnoE '(class=|class=format!\()"[^"]*"' --include='*.rs' "$SOURCE_DIR" \
  | awk -F'"' '{ n = split($2, t, " "); for (i = 1; i <= n; i++) if (t[i] !~ /^(fui-|\{)/) { print $0; break } }' || true)"
report "a screen writes a class that is not ferrohealth-ui's" \
  "the ferrohealth-ui class or component that lays it out" \
  "$utilities"

if [[ $failures -gt 0 ]]; then
  echo "console-tokens: $failures rule(s) broken."
  exit 1
fi

echo "console-tokens: every screen names roles and steps only."
