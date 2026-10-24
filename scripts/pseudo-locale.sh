#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The console's pseudo-locale: a Fluent catalog that writes every English
# message longer, so a capture shows where a screen has no room for a longer
# translation (no specification governs this: our own design).
#
#   scripts/pseudo-locale.sh IN OUT
#   scripts/pseudo-locale.sh --self-test
#
# Reads the English catalog IN and writes OUT, in which every message keeps
# its id, its placeables and its select expressions, and its text is wrapped
# in U+27E6 and U+27E7, its vowels carry an accent, and it gains a tail of
# middle dots 40 percent of its length long. A German or Dutch label runs
# about that much longer than its English one, and the brackets show a string
# a layout cut off. The two number marks the formatter reads, `number-group`
# and `number-decimal`, are kept as they are. `scripts/ui-e2e.sh --review DIR
# --pseudo-locale` builds the console with OUT as the catalog of `en-XA`
# (English in the user-assigned region XA of ISO 3166-1, the tag Android uses
# for its accented pseudo-locale) and captures every screen in it.
set -euo pipefail

usage() {
  echo "usage: scripts/pseudo-locale.sh IN OUT | --self-test" >&2
  exit 2
}

# Writes the pseudo-locale of the catalog $1 to $2.
pseudo() {
  local in="$1" out="$2"
  perl -CSD -Mutf8 -e '
    use strict;
    use warnings;
    my %accent = (a => "á", e => "é", i => "í", o => "ó", u => "ú",
                  A => "Á", E => "É", I => "Í", O => "Ó", U => "Ú");
    my %keep = map { $_ => 1 } qw(number-group number-decimal);
    # Text outside the placeables of one pattern segment: accented, and
    # counted, so the tail can be 40 percent of it.
    sub stretch {
      my ($segment) = @_;
      my ($depth, $out, $letters) = (0, "", 0);
      for my $c (split //, $segment) {
        if ($c eq "{") { $depth++; $out .= $c; next; }
        if ($c eq "}") { $depth-- if $depth > 0; $out .= $c; next; }
        if ($depth == 0) {
          $letters++ if $c =~ /\S/;
          $out .= exists $accent{$c} ? $accent{$c} : $c;
        } else {
          $out .= $c;
        }
      }
      return $segment if $letters == 0 || $depth != 0;
      my $tail = "·" x int(($letters * 2 + 4) / 5);
      return "\x{27E6}" . $out . $tail . "\x{27E7}";
    }
    my $skip = 0;
    while (my $line = <STDIN>) {
      chomp $line;
      if ($line =~ /^([a-z][a-z0-9-]*) = ?(.*)$/) {
        $skip = $keep{$1} ? 1 : 0;
        print($skip || $2 eq "" ? "$line\n" : "$1 = " . stretch($2) . "\n");
      } elsif ($skip || $line =~ /^\s*(#.*)?$/ || $line =~ /^\s*[{}]/) {
        print "$line\n";
      } elsif ($line =~ /^(\s+\.[a-z][a-z0-9-]* = ?)(.*)$/) {
        print $1 . stretch($2) . "\n";
      } elsif ($line =~ /^(\s+\*?\[[^\]]+\] ?)(.*)$/) {
        print $1 . stretch($2) . "\n";
      } elsif ($line =~ /^(\s+)(.*)$/) {
        print $1 . stretch($2) . "\n";
      } else {
        print "$line\n";
      }
    }
  ' < "$in" > "$out"
}

self_test() {
  local scratch fails=0
  scratch="$(mktemp -d)"
  trap 'rm -rf "$scratch"' RETURN
  cat > "$scratch/en.ftl" << 'EOF'
# A comment stays.
number-group = ,
number-decimal = .
plain = Code systems
placed = Concepts { $shown } of { $total }.
noun = { $count ->
        [one] code system
       *[other] code systems
    }
    .empty = No code systems
long = The first line
    and the second.
EOF
  pseudo "$scratch/en.ftl" "$scratch/out.ftl"
  # expect NAME WANT_LINE
  expect() {
    if grep -qxF -- "$2" "$scratch/out.ftl"; then
      echo "ok: $1"
    else
      echo "FAIL: $1: no line '$2' in:" >&2
      cat "$scratch/out.ftl" >&2
      fails=$((fails + 1))
    fi
  }
  expect "a comment stays" "# A comment stays."
  expect "the group mark stays" "number-group = ,"
  expect "the decimal mark stays" "number-decimal = ."
  expect "a plain message is accented, padded and bracketed" "plain = ⟦Códé systéms·····⟧"
  expect "a placeable stays whole" "placed = ⟦Cóncépts { \$shown } óf { \$total }.·····⟧"
  expect "a selector line stays" "noun = { \$count ->"
  expect "a variant's text is stretched" "        [one] ⟦códé systém····⟧"
  expect "the default variant's text is stretched" "       *[other] ⟦códé systéms·····⟧"
  expect "a closing brace stays" "    }"
  expect "an attribute's text is stretched" "    .empty = ⟦Nó códé systéms······⟧"
  expect "a continuation line is stretched" "    ⟦ánd thé sécónd.······⟧"
  if [[ "$fails" -ne 0 ]]; then
    echo "pseudo-locale: self-test FAILED ($fails)" >&2
    return 1
  fi
  echo "pseudo-locale: self-test OK."
}

case "${1:-}" in
  --self-test)
    [[ $# -eq 1 ]] || usage
    self_test
    ;;
  *)
    [[ $# -eq 2 ]] || usage
    pseudo "$1" "$2"
    ;;
esac
