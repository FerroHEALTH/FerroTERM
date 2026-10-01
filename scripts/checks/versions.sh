#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# scripts/checks/versions.sh
#
# Version-drift guard: docs/VERSIONS.md is the single source of truth, and this
# script fails when a config file disagrees with it. It runs on a discovery-phase
# repo, so every check SKIPS LOUDLY (not fails) when the file it compares does not
# exist yet, and activates automatically as the project grows. Run locally or in
# CI (the `versions` job).
#
# Exit 0 = all present checks agree (skips are fine). Exit 1 = a real drift.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

fail=0
note() { printf '  %s\n' "$*"; }
bad() { printf '  DRIFT: %s\n' "$*" >&2; fail=1; }

# --- the server bin is named `ferroterm`, as release-build.yml packages it -------
echo "== server binary name (app/ferroterm-server/Cargo.toml <-> release-build.yml)"
if [[ -f app/ferroterm-server/Cargo.toml ]]; then
  bin_name="$(awk '/^\[\[bin\]\]/{f=1;next} /^\[/{f=0} f && /^name[[:space:]]*=/{gsub(/[" ]/,""); sub(/^name=/,""); print; exit}' app/ferroterm-server/Cargo.toml || true)"
  if [[ "$bin_name" != "ferroterm" ]]; then
    bad "app/ferroterm-server/Cargo.toml [[bin]] name is '${bin_name:-unset}', release-build.yml packages 'ferroterm'"
  else
    note "OK: the bin is ferroterm"
  fi
else
  note "no app/ferroterm-server/Cargo.toml yet — skipped"
fi

# --- compose.yaml image tag default == root Cargo.toml workspace version --------
echo "== quickstart image tag (compose.yaml <-> Cargo.toml)"
if [[ -f compose.yaml ]] && [[ -f Cargo.toml ]]; then
  compose_ver="$(grep -m1 'image: ghcr.io/ferrohealth/ferroterm:' compose.yaml | sed -E 's/.*:-([^}]*)\}.*/\1/' | tr -d '[:space:]' || true)"
  cargo_ver_c="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version[[:space:]]*=/{gsub(/[" ]/,""); sub(/^version=/,""); print; exit}' Cargo.toml || true)"
  if [[ -z "$compose_ver" ]]; then
    bad "compose.yaml has no ghcr.io/ferrohealth/ferroterm image tag default"
  elif [[ "$compose_ver" != "$cargo_ver_c" ]]; then
    bad "compose.yaml image tag default ($compose_ver) != Cargo.toml workspace version ($cargo_ver_c)"
  else
    note "OK: the quickstart pulls $compose_ver"
  fi
else
  note "no compose.yaml yet — skipped"
fi

# --- compose.sync.yaml image tag default == the workspace version -------------
echo "== sync image tag (compose.sync.yaml <-> Cargo.toml)"
if [[ -f compose.sync.yaml ]] && [[ -f Cargo.toml ]]; then
  sync_ver="$(grep -m1 'image: ghcr.io/ferrohealth/ferroterm-sync:' compose.sync.yaml | sed -E 's/.*:-([^}]*)\}.*/\1/' | tr -d '[:space:]' || true)"
  cargo_ver_s="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version[[:space:]]*=/{gsub(/[" ]/,""); sub(/^version=/,""); print; exit}' Cargo.toml || true)"
  if [[ -z "$sync_ver" ]]; then
    bad "compose.sync.yaml has no ghcr.io/ferrohealth/ferroterm-sync image tag default"
  elif [[ "$sync_ver" != "$cargo_ver_s" ]]; then
    bad "compose.sync.yaml image tag default ($sync_ver) != Cargo.toml workspace version ($cargo_ver_s)"
  else
    note "OK: the sync overlay pulls $sync_ver"
  fi
else
  note "no compose.sync.yaml yet — skipped"
fi

# --- CITATION.cff version == root Cargo.toml [workspace.package] version -------
echo "== product version (CITATION.cff <-> Cargo.toml)"
cff_ver=""
[[ -f CITATION.cff ]] && cff_ver="$(grep -m1 '^version:' CITATION.cff | sed -E 's/^version:[[:space:]]*//; s/["'\'']//g' | tr -d '[:space:]' || true)"
if [[ -f Cargo.toml ]]; then
  # The version line inside the [workspace.package] table.
  cargo_ver="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version[[:space:]]*=/{gsub(/[" ]/,""); sub(/^version=/,""); print; exit}' Cargo.toml || true)"
  if [[ -z "$cff_ver" ]]; then
    bad "Cargo.toml exists but CITATION.cff has no version"
  elif [[ -z "$cargo_ver" ]]; then
    note "Cargo.toml has no [workspace.package] version yet — skipped"
  elif [[ "$cff_ver" != "$cargo_ver" ]]; then
    bad "CITATION.cff version ($cff_ver) != Cargo.toml workspace version ($cargo_ver)"
  else
    note "OK: both are $cff_ver"
  fi
else
  note "no Cargo.toml yet — skipped (CITATION.cff version: ${cff_ver:-none})"
fi

# A benchmark record states the version that produced it, and the rendered
# blocks (bench-table.sh) repeat it, so a measurement taken with an earlier
# release is history rather than a stale claim. Every `file:line` inside a
# rendered block is dropped from the scan above.
drop_rendered_benchmarks() {
  local hit file line
  while IFS= read -r hit; do
    file="${hit%%:*}"
    line="${hit#*:}"
    line="${line%%:*}"
    if awk -v n="$line" '
      /<!-- bench-(table|figures):begin -->/ { start = NR }
      /<!-- bench-(table|figures):end -->/ { if (start && n > start && n < NR) { found = 1 } ; start = 0 }
      END { exit found ? 0 : 1 }
    ' "$file"; then
      continue
    fi
    printf '%s\n' "$hit"
  done
}

# --- No stale product version anywhere the release is named -------------------
# Every file that names the release (the README, the landing page, the book,
# the compose file, the citation file, the version matrix) must name the
# current one. A version below the workspace version is stale; a higher one is
# the roadmap and is allowed. The changelog and the lock file keep history.
echo "== stale product versions (README, website, docs <-> Cargo.toml)"
if [[ -f Cargo.toml ]]; then
  current="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version[[:space:]]*=/{gsub(/[" ]/,""); sub(/^version=/,""); print; exit}' Cargo.toml || true)"
  if [[ -n "$current" ]]; then
    stale=0
    while IFS= read -r hit; do
      [[ -n "$hit" ]] || continue
      found="${hit##*:}"
      lowest="$(printf '%s\n%s\n' "$found" "$current" | sort -V | head -n1)"
      if [[ "$found" != "$current" ]] && [[ "$lowest" = "$found" ]]; then
        bad "stale version $found (current is $current) at ${hit%:*}"
        stale=1
      fi
    done < <(git grep -n -o -E '(^|[^0-9.])0\.0\.[0-9]+([^0-9.]|$)' -- README.md CITATION.cff compose.yaml compose.sync.yaml docs website/landing website/book/src \
      | sed -E 's/^([^:]+:[0-9]+):.*[^0-9.]?(0\.0\.[0-9]+).*$/\1:\2/' \
      | drop_rendered_benchmarks || true)
    [[ "$stale" -eq 0 ]] && note "OK: every release mention is $current or a later milestone"
  else
    note "Cargo.toml has no [workspace.package] version yet — skipped"
  fi
else
  note "no Cargo.toml yet — skipped"
fi

# --- Every command that names the release names the current one ---------------
# The check above matches a bare version anywhere, so it stays on 0.0.x: a
# sentence may name an older release on purpose (an audit of v0.1.0, "what
# stable means in v0.1.0"), and a dependency has a version of its own. A
# command a reader is meant to run is different: an image tag, a release
# download, or an attestation over a named asset all claim to be the current
# release, and a stale one sends the reader to the wrong artifact.
echo "== release-naming commands (README, docs, book, compose <-> Cargo.toml)"
if [[ -n "${current:-}" ]]; then
  named=0
  while IFS= read -r hit; do
    [[ -n "$hit" ]] || continue
    bad "names release ${hit##*:} where the current one is $current at ${hit%:*}"
    named=1
  done < <(git grep -n -o -E "(ferroterm:|ferroterm-v|release download v)0\.[0-9]+\.[0-9]+" \
    -- README.md compose.yaml compose.sync.yaml docs website/landing website/book/src \
    | sed -E "s/^([^:]+:[0-9]+):.*[^0-9.]([0-9]+\.[0-9]+\.[0-9]+)$/\1:\2/" \
    | grep -v ":${current}$" || true)
  [[ "$named" -eq 0 ]] && note "OK: every image tag, download, and attestation names $current"
fi

# --- The vendored ECL grammar tag == docs/VERSIONS.md ECL pin ------------------
echo "== vendored ECL grammar (PROVENANCE.md <-> docs/VERSIONS.md)"
ecl_prov="crates/sct-ecl/vendor/PROVENANCE.md"
if [[ -f "$ecl_prov" ]]; then
  ecl_tag="$(sed -nE 's/^- Tag:[[:space:]]*//p' "$ecl_prov" | head -n1 | tr -d '[:space:]')"
  ecl_pin="$(awk -F'|' '$2 ~ /^[[:space:]]*ECL[[:space:]]*$/ { v = $3; gsub(/^[[:space:]]+/, "", v); split(v, w, /[[:space:]]/); print w[1]; exit }' docs/VERSIONS.md)"
  if [[ -z "$ecl_tag" ]]; then
    bad "$ecl_prov has no '- Tag:' line"
  elif [[ -z "$ecl_pin" ]]; then
    bad "docs/VERSIONS.md has no ECL row"
  elif [[ "$ecl_tag" != "$ecl_pin" ]]; then
    bad "ECL grammar: PROVENANCE.md says $ecl_tag, docs/VERSIONS.md pins $ecl_pin"
  else
    note "OK: ECL grammar $ecl_tag"
  fi
else
  note "no vendored ECL grammar yet — skipped"
fi

# --- The vendored SCG syntax commit == docs/VERSIONS.md SCG pin --------------
echo "== vendored SCG syntax (PROVENANCE.md <-> docs/VERSIONS.md)"
scg_prov="crates/sct-scg/vendor/PROVENANCE.md"
if [[ -f "$scg_prov" ]]; then
  scg_commit="$(sed -nE 's/^- Commit:[[:space:]]*//p' "$scg_prov" | head -n1 | tr -d '[:space:]')"
  scg_pin="$(awk -F'|' '$2 ~ /^[[:space:]]*SCG[[:space:]]*$/ { v = $3; gsub(/^[[:space:]]+/, "", v); split(v, w, /[[:space:]]/); print w[1]; exit }' docs/VERSIONS.md)"
  if [[ -z "$scg_commit" ]]; then
    bad "$scg_prov has no '- Commit:' line"
  elif [[ -z "$scg_pin" ]]; then
    bad "docs/VERSIONS.md has no SCG row"
  elif [[ "$scg_commit" != "$scg_pin" ]]; then
    bad "SCG syntax: PROVENANCE.md says $scg_commit, docs/VERSIONS.md pins $scg_pin"
  else
    note "OK: SCG syntax ${scg_commit:0:12}"
  fi
else
  note "no vendored SCG syntax yet — skipped"
fi

# --- One licence everywhere the project names its own ------------------------
# The project's own code is BUSL-1.1 (LICENSE); a header, manifest, badge, or
# image label still saying MIT or Apache-2.0 is a stale claim. Third-party files keep theirs.
echo "== licence (LICENSE <-> SPDX headers, manifests, badges, labels)"
if [[ -f LICENSE ]]; then
  stale=0
  if ! grep -q 'Business Source License 1.1' LICENSE; then
    bad "LICENSE is not the Business Source License 1.1"; stale=1
  fi
  while IFS= read -r hit; do
    [[ -n "$hit" ]] || continue
    bad "stale licence claim at $hit"; stale=1
  done < <(git grep -n -E 'SPDX-License-Identifier: (MIT|Apache-2\.0)|License-MIT|License-Apache|^license = "(MIT|Apache-2\.0)"|^license: (MIT|Apache-2\.0)|image\.licenses="?(MIT|Apache)' \
    -- ':!scripts/checks/versions.sh' ':!crates/sct-ecl/vendor' ':!crates/rf2/LICENSE' ':!crates/rf2/Cargo.toml' ':!crates/rf2/README.md' ':!website/book/mermaid.min.js' ':!CHANGELOG.md' || true)
  # rf2 is published under Apache 2.0 (the owner's decision, #223); it carries
  # the Apache text and says so.
  if ! grep -q '^license = "Apache-2.0"' crates/rf2/Cargo.toml || ! grep -q 'Apache License' crates/rf2/LICENSE; then
    bad "crates/rf2 is not Apache-2.0 in its manifest and LICENSE"; stale=1
  fi
  [[ "$stale" -eq 0 ]] && note "OK: the project's own files name BUSL-1.1"
else
  note "no LICENSE yet — skipped"
fi

# --- Conformance suite pin: the suite script and the badges read the same commit
echo "== conformance suite (tx-ecosystem.sh <-> docs/VERSIONS.md)"
if [[ -f scripts/checks/tx-ecosystem.sh ]]; then
  script_commit="$(grep -m1 '^SUITE_COMMIT=' scripts/checks/tx-ecosystem.sh | sed 's/^SUITE_COMMIT=//' | tr -d '[:space:]')"
  table_commit="$(grep -oE 'fhir-tx-ecosystem-ig. at .[0-9a-f]{40}' docs/VERSIONS.md | head -1 | grep -oE '[0-9a-f]{40}' || true)"
  if [[ -z "$table_commit" ]]; then
    bad "docs/VERSIONS.md names no tx-ecosystem suite commit"
  elif [[ "$script_commit" != "$table_commit" ]]; then
    bad "tx-ecosystem.sh pins suite commit $script_commit, docs/VERSIONS.md says $table_commit"
  else
    note "OK: the suite commit is $table_commit"
  fi
  script_validator="$(grep -m1 '^VALIDATOR_VERSION=' scripts/checks/tx-ecosystem.sh | sed 's/^VALIDATOR_VERSION=//' | tr -d '[:space:]')"
  script_sha="$(grep -m1 '^VALIDATOR_SHA256=' scripts/checks/tx-ecosystem.sh | sed 's/^VALIDATOR_SHA256=//' | tr -d '[:space:]')"
  table_validator="$(grep -oE 'FHIR Validator .[^|]*\| [0-9]+\.[0-9]+\.[0-9]+' docs/VERSIONS.md | grep -oE '[0-9]+\.[0-9]+\.[0-9]+$' || true)"
  if [[ -z "$table_validator" ]]; then
    bad "docs/VERSIONS.md names no FHIR Validator version"
  elif [[ "$script_validator" != "$table_validator" ]]; then
    bad "tx-ecosystem.sh pins validator $script_validator, docs/VERSIONS.md says $table_validator"
  elif ! grep -qF "$script_sha" docs/VERSIONS.md; then
    bad "tx-ecosystem.sh pins validator sha256 $script_sha, docs/VERSIONS.md names another"
  else
    note "OK: the validator is $table_validator"
  fi
  [[ -f conformance/tx-ecosystem/total.txt ]] || bad "conformance/tx-ecosystem/total.txt is missing (the badges need the suite total)"
  grep -qE 'test cases [0-9]+\.[0-9]+\.[0-9]+' docs/VERSIONS.md || bad "docs/VERSIONS.md names no suite test-case version (the badge label needs it)"
else
  note "no scripts/checks/tx-ecosystem.sh yet — skipped"
fi

# --- Published crates: metadata, lockstep line (.claude/rules/crates-publishing.md)
echo "== published crates (crates/*/Cargo.toml <-> README, LICENSE, root requirements)"
if ls crates/*/Cargo.toml >/dev/null 2>&1; then
  line="$(awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' crates/rf2/Cargo.toml || true)"
  # The one published list is the CRATES array of scripts/release/publish-crates.sh
  # (dependency order for the upload). A crates/* member is published when it
  # is in that list; a member outside it must say `publish = false` and carry
  # a `# NOTE: unpublished:` reason line, so a forgotten member and a held-out
  # member can never look alike.
  published="$(awk '/^readonly CRATES=\(/{p=1; next} p && /^\)/{exit} p {gsub(/^[ \t]+|[ \t]+$/, ""); if ($0 != "") print}' scripts/release/publish-crates.sh)"
  for name in $published; do
    [[ -f "crates/$name/Cargo.toml" ]] || bad "publish-crates.sh lists $name, which is not a crates/* member"
  done
  for manifest in crates/*/Cargo.toml; do
    dir="$(dirname "$manifest")"
    name="$(awk -F'"' '/^\[package\]/{p=1} p && /^name = /{print $2; exit}' "$manifest")"
    if ! grep -qx "$name" <<<"$published"; then
      grep -q '^publish = false' "$manifest" || bad "$manifest is not in publish-crates.sh and does not say publish = false"
      grep -q '^# NOTE: unpublished:' "$manifest" || bad "$manifest is unpublished without a '# NOTE: unpublished:' reason"
      continue
    fi
    [[ -f "$dir/README.md" ]] || bad "$dir has no README.md (published crate)"
    [[ -f "$dir/LICENSE" ]] || bad "$dir has no LICENSE (published crate)"
    case "$dir" in
      # the Apache-2.0 crate carries the Apache text, checked above
      crates/rf2) ;;
      *) cmp -s LICENSE "$dir/LICENSE" || bad "$dir/LICENSE differs from the root LICENSE" ;;
    esac
    grep -q '^publish = true' "$manifest" || bad "$manifest is not publish = true"
    grep -q '^readme = "README.md"' "$manifest" || bad "$manifest names no readme"
    grep -q '^description = ' "$manifest" || bad "$manifest has no description"
    ver="$(awk -F'"' '/^\[package\]/{p=1} p && /^version = /{print $2; exit}' "$manifest")"
    [[ "$ver" = "$line" ]] || bad "$manifest is at $ver, the crate line is $line"
    grep -qE "^${name} = \{ path = \"crates/${name}\", version = \"${line}\" \}" Cargo.toml \
      || bad "root Cargo.toml does not require $name at $line"
  done
  note "OK: every crate in publish-crates.sh has README, LICENSE, publish = true, and sits on the $line line; every other member is declared unpublished"
else
  note "no crates/*/Cargo.toml yet — skipped"
fi

# --- rust-toolchain.toml present (sanity; channel recorded in VERSIONS.md) -----
echo "== rust toolchain"
if [[ -f rust-toolchain.toml ]]; then
  chan="$(grep -m1 -E '^channel[[:space:]]*=' rust-toolchain.toml | sed -E 's/.*=[[:space:]]*//; s/["'\'']//g' | tr -d '[:space:]' || true)"
  note "rust-toolchain.toml channel: ${chan:-unset}"
else
  note "no rust-toolchain.toml yet — skipped (lands with the workspace)"
fi

# --- The FHIR model pin (docs/VERSIONS.md <-> Cargo.toml <-> Cargo.lock) ------
# `fhir-types` is generated and published by the FerroBRIDGE repository and
# consumed from crates.io, so this pin replaces the vendored-package check. The
# requirement is a caret, so a newer 0.1.x release resolves into Cargo.lock on
# its own (a Dependabot bump, a `cargo update`) and would otherwise change the
# served FHIR model with nothing recording it. All three must name one version.
echo "== FHIR model pin (docs/VERSIONS.md <-> Cargo.toml <-> Cargo.lock)"
if [[ -f Cargo.toml ]] && [[ -f docs/VERSIONS.md ]] && [[ -f Cargo.lock ]]; then
  req="$(sed -nE 's/^fhir-types = "([^"]+)".*/\1/p' Cargo.toml | head -n1)"
  pin="$(awk -F'|' '$2 ~ /^[[:space:]]*`fhir-types`[[:space:]]*$/ { v = $3; gsub(/^[[:space:]]+/, "", v); split(v, w, /[[:space:]]/); print w[1]; exit }' docs/VERSIONS.md)"
  locked="$(awk '$1 == "name" && $3 == "\"fhir-types\"" { hit = 1; next } hit && $1 == "version" { gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
  if [[ -z "$req" ]]; then
    bad "root Cargo.toml has no 'fhir-types = \"<version>\"' requirement"
  elif [[ -z "$pin" ]]; then
    bad "docs/VERSIONS.md has no \`fhir-types\` row"
  elif [[ "$req" != "$pin" ]]; then
    bad "root Cargo.toml requires fhir-types $req, docs/VERSIONS.md pins $pin"
  elif [[ -z "$locked" ]]; then
    bad "Cargo.lock records no fhir-types version"
  elif [[ "$locked" != "$pin" ]]; then
    bad "Cargo.lock resolves fhir-types $locked, docs/VERSIONS.md pins $pin; move the pin and the requirement with the lock"
  else
    note "OK: fhir-types $pin (the requirement, the lock, and the pin table agree)"
  fi
else
  note "no Cargo.toml, Cargo.lock, or docs/VERSIONS.md yet — skipped"
fi

# --- the viewer tool pins (docs/VERSIONS.md <-> Trunk.toml <-> the workflows) ---
echo "== viewer tool pins (docs/VERSIONS.md <-> Trunk.toml <-> the workflows)"
if [[ -f app/ferroterm-viewer/Trunk.toml ]] && [[ -f docs/VERSIONS.md ]]; then
  pin_trunk="$(sed -n 's/^| Trunk | \([0-9][0-9.]*\),.*/\1/p' docs/VERSIONS.md | head -n1)"
  pin_leptosfmt="$(sed -n 's/^| .leptosfmt. | \([0-9][0-9.]*\),.*/\1/p' docs/VERSIONS.md | head -n1)"
  pin_tailwind="$(sed -n 's/^| Tailwind CSS standalone CLI | \([0-9][0-9.]*\),.*/\1/p' docs/VERSIONS.md | head -n1)"
  toml_trunk="$(sed -n 's/^trunk-version[[:space:]]*=[[:space:]]*">=\([0-9][0-9.]*\)"/\1/p' app/ferroterm-viewer/Trunk.toml | head -n1)"
  toml_tailwind="$(sed -n 's/^tailwindcss[[:space:]]*=[[:space:]]*"\([0-9][0-9.]*\)"/\1/p' app/ferroterm-viewer/Trunk.toml | head -n1)"
  if [[ -z "$pin_trunk" || -z "$pin_leptosfmt" || -z "$pin_tailwind" ]]; then
    bad "docs/VERSIONS.md has no Viewer toolchain rows for Trunk, leptosfmt, and the Tailwind CLI"
  else
    [[ "$toml_trunk" == "$pin_trunk" ]] \
      || bad "Trunk.toml trunk-version floor (${toml_trunk:-unset}) != docs/VERSIONS.md pin ($pin_trunk)"
    [[ "$toml_tailwind" == "$pin_tailwind" ]] \
      || bad "Trunk.toml [tools] tailwindcss (${toml_tailwind:-unset}) != docs/VERSIONS.md pin ($pin_tailwind)"
    # Every workflow that installs a viewer tool installs the pinned version.
    for wf in .github/workflows/ci.yml .github/workflows/release-build.yml; do
      [[ -f "$wf" ]] || continue
      while read -r got; do
        [[ "$got" == "$pin_trunk" ]] || bad "$wf installs trunk@$got, docs/VERSIONS.md pins $pin_trunk"
      done < <(sed -n 's/.*tool:[^#]*trunk@\([0-9][0-9.]*\).*/\1/p' "$wf")
      while read -r got; do
        [[ "$got" == "$pin_leptosfmt" ]] || bad "$wf installs leptosfmt $got, docs/VERSIONS.md pins $pin_leptosfmt"
      done < <(sed -n 's/.*cargo install leptosfmt .*--version \([0-9][0-9.]*\).*/\1/p' "$wf")
    done
    [[ "$fail" -ne 0 ]] || note "OK: Trunk $pin_trunk, Tailwind CLI $pin_tailwind, leptosfmt $pin_leptosfmt"
  fi
else
  note "no viewer crate yet — skipped"
fi

# --- the release lane's viewer feature exists on the server ---------------------
echo "== viewer embed feature (release-build.yml <-> app/ferroterm-server/Cargo.toml)"
if [[ -f .github/workflows/release-build.yml ]] && [[ -f app/ferroterm-server/Cargo.toml ]]; then
  want_feature="$(sed -n 's/^[[:space:]]*VIEWER_FEATURE:[[:space:]]*\([A-Za-z0-9_-]*\).*/\1/p' .github/workflows/release-build.yml | head -n1)"
  server_features="$(awk '/^\[features\]/{f=1;next} /^\[/{f=0} f && /^[A-Za-z0-9_-]+[[:space:]]*=/{sub(/[[:space:]]*=.*/,""); print}' app/ferroterm-server/Cargo.toml)"
  if [[ -z "$want_feature" ]]; then
    bad "release-build.yml has no VIEWER_FEATURE, so the release lane names no embed feature"
  elif [[ -z "$server_features" ]]; then
    note "app/ferroterm-server declares no features yet — skipped (the lane wants '$want_feature')"
  elif ! printf '%s\n' "$server_features" | grep -qx "$want_feature"; then
    bad "release-build.yml builds --features $want_feature, which app/ferroterm-server/Cargo.toml does not declare"
  else
    note "OK: the release lane builds the server with its '$want_feature' feature"
  fi
else
  note "no release-build.yml or no server crate yet — skipped"
fi

echo
if [[ "$fail" -ne 0 ]]; then
  echo "versions: DRIFT detected (see above)." >&2
  exit 1
fi
echo "versions: OK (all present checks agree)."
