#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The differential check against Snowstorm, the reference server, over one
# licensed SNOMED CT edition.
#
#   scripts/checks/differential.sh --index DIR [--snowstorm URL] [--server URL]
#                                  [--fhir r4|r4b|r5] [--out DIR] [--requests FILE]
#
# Both servers answer the same requests over the same edition and the harness
# diffs a projection of each answer. A divergence is a defect in FerroTERM or a
# recorded, spec-cited deviation, never a silent difference: where Snowstorm and
# the specification disagree the specification wins, and the divergence is worth
# a note (.claude/rules/testing.md, oracles).
#
# The two servers answer the same question in different shapes, so the raw
# bodies are never compared. Each request names the projection to diff, and the
# projections below are the answer, not the wire form:
#
#   display   the display `$lookup` returns
#   result    the boolean `$validate-code` returns
#   outcome   the code `$subsumes` returns
#   codes     the sorted set of codes an expansion contains, whichever implicit
#             value set an `expand` request names (`ecl`, `isa`, or the raw
#             `vs` form such as `refset`)
#   targets   the sorted set of `system|code` a translation matches
#
# Without --server the script starts target/release/ferroterm on 127.0.0.1:8099
# over --index (build it first: cargo build --release -p ferroterm-server).
# --snowstorm is the base of a running Snowstorm FHIR endpoint, for example
# http://127.0.0.1:8080/fhir; the harness never starts one, because a Snowstorm
# deployment wants Elasticsearch and 16 to 32 GB of RAM.
#
# The request sample carries SNOMED CT identifiers and nothing else, so it
# distributes no licensed content (.claude/rules/vendored-inputs.md).
set -euo pipefail

# Seconds to wait for the started server to answer /health. Generous because it
# covers reading an edition, and safe because the wait ends the moment the
# server exits.
READY_TIMEOUT=${FERROTERM_READY_TIMEOUT:-300}

index=""
snowstorm=""
server=""
fhir=r4b
out=target/differential
requests=conformance/differential/requests.json

while [[ $# -gt 0 ]]; do
  case "$1" in
    --index) index=$2; shift 2 ;;
    --snowstorm) snowstorm=$2; shift 2 ;;
    --server) server=$2; shift 2 ;;
    --fhir) fhir=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --requests) requests=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$fhir" in
  r4|r4b|r5) ;;
  *) echo "--fhir must be r4, r4b, or r5, not '$fhir'" >&2; exit 2 ;;
esac
if [[ -z "$snowstorm" ]]; then
  echo "differential: --snowstorm is required; the harness never starts one" >&2
  exit 2
fi
if [[ ! -f "$requests" ]]; then
  echo "differential: no request sample at $requests" >&2
  exit 2
fi

started=""
if [[ -z "$server" ]]; then
  if [[ -z "$index" ]]; then
    echo "differential: --index names the edition to serve, or --server names a running one" >&2
    exit 2
  fi
  mkdir -p "$(dirname "$out")"
  FERROTERM_USAGE_REPORT=off FERROTERM_INDEX="$index" FERROTERM_LISTEN=127.0.0.1:8099 FERROTERM_LOG_FORMAT=json \
    target/release/ferroterm > "$out.server.log" 2>&1 &
  started=$!
  trap 'kill "$started" 2>/dev/null || true' EXIT
  ready=""
  # The loop used to fall out of its twenty seconds without saying so, and the
  # run then compared two servers when one of them was not running. Reading an
  # artifact takes as long as it takes, and a server that dies is noticed at
  # once rather than at the deadline (#425, #493).
  for _ in $(seq 1 "$((READY_TIMEOUT * 5))"); do
    if ! kill -0 "$started" 2>/dev/null; then
      echo "differential: the server exited before it was ready. It said:" >&2
      cat "$out.server.log" >&2
      exit 1
    fi
    if curl -sf "http://127.0.0.1:8099/health" >/dev/null 2>&1; then
      ready=1
      break
    fi
    sleep 0.2
  done
  if [[ -z "$ready" ]]; then
    echo "differential: the server did not answer /health within ${READY_TIMEOUT}s. It said:" >&2
    cat "$out.server.log" >&2
    exit 1
  fi
  server="http://127.0.0.1:8099/$fhir"
  # A refused artifact leaves its system unserved, and every difference this
  # run then reports is a difference from nothing (#493).
  scripts/checks/served-artifacts.sh --server "$server" --index "$index"
fi

rm -rf "$out"
mkdir -p "$out"
system=$(jq -r '.system' "$requests")

# The version URI each server reports. It names the edition's module and the
# release date
# (<https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard/2-snomed-ct-uri-space>).
# The DATE identifies the content: two servers on different dates hold
# different concepts and nothing below would compare behaviour, so that stops
# the run. The MODULE is an answer the two servers compute, so a difference
# there is a divergence like any other and is recorded, not a reason to stop.
version_uri="$server/CodeSystem/\$lookup?system=$system&code=138875005"
ours=$(curl -sf "$version_uri" \
  | jq -r '[.parameter[]? | select(.name == "version") | .valueString] | first // "unknown"')
theirs=$(curl -sf "$snowstorm/CodeSystem/\$lookup?system=$system&code=138875005" \
  | jq -r '[.parameter[]? | select(.name == "version") | .valueString] | first // "unknown"')
echo "differential: FerroTERM serves $ours"
echo "differential: Snowstorm serves $theirs"
if [[ "${ours##*/}" != "${theirs##*/}" ]]; then
  echo "differential: the two servers serve different releases; load the same release in both" >&2
  exit 1
fi

# One field of a request, percent-encoded for a query string. A concept id
# passes through unchanged; a post-coordinated expression carries spaces and
# operators that would otherwise cut the query short.
field() {
  jq -rR '@uri' <<<"$(jq -r ".$1" <<<"$2")"
}

# The URL of one request against one base, per operation.
url_of() {
  local base=$1 operation=$2 parameters=$3 query
  case "$operation" in
    lookup)
      query="system=$system&code=$(field code "$parameters")"
      printf "%s/CodeSystem/\$lookup?%s" "$base" "$query"
      ;;
    validate-code)
      query="url=$system&code=$(field code "$parameters")"
      local display
      display=$(jq -r '.display // empty' <<<"$parameters")
      [[ -n "$display" ]] && query="$query&display=$(jq -rR '@uri' <<<"$display")"
      printf "%s/CodeSystem/\$validate-code?%s" "$base" "$query"
      ;;
    subsumes)
      query="system=$system&codeA=$(field codeA "$parameters")&codeB=$(field codeB "$parameters")"
      printf "%s/CodeSystem/\$subsumes?%s" "$base" "$query"
      ;;
    expand)
      local ecl isa vs count
      count=$(jq -r '.count // "100"' <<<"$parameters")
      ecl=$(jq -r '.ecl // empty' <<<"$parameters")
      isa=$(jq -r '.isa // empty' <<<"$parameters")
      if [[ -n "$ecl" ]]; then
        vs="$system?fhir_vs=ecl/$(jq -rR '@uri' <<<"$ecl")"
      elif [[ -n "$isa" ]]; then
        vs="$system?fhir_vs=isa/$isa"
      else
        vs="$system?fhir_vs=$(jq -r '.vs' <<<"$parameters")"
      fi
      printf "%s/ValueSet/\$expand?url=%s&count=%s" "$base" "$(jq -rR '@uri' <<<"$vs")" "$count"
      ;;
    translate)
      local cm
      cm=$(jq -r '.cm' <<<"$parameters")
      query="url=$(jq -rR '@uri' <<<"$system?fhir_cm=$cm")&system=$system&code=$(jq -r '.code' <<<"$parameters")"
      printf "%s/ConceptMap/\$translate?%s" "$base" "$query"
      ;;
    *)
      return 1
      ;;
  esac
}

# The projection of one answer, by the comparison the request names.
project() {
  local compare=$1
  case "$compare" in
    display)
      jq -r '[.parameter[]? | select(.name == "display") | .valueString] | first // "«none»"'
      ;;
    result)
      jq -r '[.parameter[]? | select(.name == "result") | .valueBoolean] | first
             | if . == null then "«none»" else tostring end'
      ;;
    outcome)
      jq -r '[.parameter[]? | select(.name == "outcome") | .valueCode] | first // "«none»"'
      ;;
    codes)
      # A server may nest `contains`, so the set of codes is read through the
      # whole tree (<https://hl7.org/fhir/R4B/valueset-operation-expand.html>).
      jq -r '[.expansion | recurse(.contains[]?) | .code? // empty]
             | sort | unique | join(",")'
      ;;
    targets)
      jq -r '[.parameter[]? | select(.name == "match") | .part[]?
              | select(.name == "concept") | .valueCoding
              | "\(.system // "")|\(.code // "")"] | sort | unique | join(",")'
      ;;
    *)
      echo "«unknown projection $compare»"
      ;;
  esac
}

total=0
diverged=0
hollow=""
: > "$out/divergences.txt"
echo "[]" > "$out/divergences.json"

# One divergence, in both the readable log and the machine-readable report.
record() {
  local name=$1 operation=$2 compare=$3 ours_value=$4 theirs_value=$5 url=$6
  diverged=$((diverged + 1))
  {
    printf '%s (%s, %s)\n' "$name" "$operation" "$compare"
    printf '  FerroTERM: %s\n' "${ours_value:0:400}"
    printf '  Snowstorm: %s\n' "${theirs_value:0:400}"
    printf '  request:   %s\n' "$url"
  } >> "$out/divergences.txt"
  jq --arg n "$name" --arg o "$operation" --arg c "$compare" \
     --arg a "$ours_value" --arg b "$theirs_value" --arg u "$url" \
     '. + [{name: $n, operation: $o, compare: $c, ferroterm: $a, snowstorm: $b, request: $u}]' \
     "$out/divergences.json" > "$out/divergences.json.tmp"
  mv "$out/divergences.json.tmp" "$out/divergences.json"
}

# The edition URI is an answer, so it is compared like every other request.
total=$((total + 1))
if [[ "$ours" != "$theirs" ]]; then
  record edition-version-uri lookup version "$ours" "$theirs" "$version_uri"
fi

while IFS= read -r request; do
  name=$(jq -r '.name' <<<"$request")
  operation=$(jq -r '.operation' <<<"$request")
  compare=$(jq -r '.compare' <<<"$request")
  parameters=$(jq -c '.parameters' <<<"$request")
  total=$((total + 1))

  if ! ours_url=$(url_of "$server" "$operation" "$parameters"); then
    echo "differential: $name names an operation the harness does not replay: $operation" >&2
    exit 2
  fi
  theirs_url=$(url_of "$snowstorm" "$operation" "$parameters")

  curl -sf "$ours_url" -o "$out/$name.ours.json" 2>/dev/null || echo '{}' > "$out/$name.ours.json"
  curl -sf "$theirs_url" -o "$out/$name.theirs.json" 2>/dev/null || echo '{}' > "$out/$name.theirs.json"

  ours_value=$(project "$compare" < "$out/$name.ours.json")
  theirs_value=$(project "$compare" < "$out/$name.theirs.json")

  # Two servers that both answer nothing agree while proving nothing, so a
  # hollow answer is reported even when the run passes.
  if [[ -z "$ours_value" || "$ours_value" == "«none»" ]]; then
    hollow="$hollow $name"
  fi

  if [[ "$ours_value" != "$theirs_value" ]]; then
    record "$name" "$operation" "$compare" "$ours_value" "$theirs_value" "$ours_url"
  fi
done < <(jq -c '.requests[]' "$requests")

echo "differential: $((total - diverged)) of $total requests agree over $ours"
if [[ -n "$hollow" ]]; then
  echo "differential: answered nothing, so these compare nothing:$hollow" >&2
fi
if [[ "$diverged" -gt 0 ]]; then
  echo "DIVERGENCE: $diverged of $total requests answer differently:" >&2
  cat "$out/divergences.txt" >&2
  echo "differential: a divergence is a defect in FerroTERM or a spec-cited deviation; the specification decides, not Snowstorm" >&2
  exit 1
fi
echo "differential: no divergence against Snowstorm"
