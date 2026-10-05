#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Reads the Nictiz Nationale Terminologieserver (NTS) syndication feed with
# YOUR account and reports, per entry, the category the service publishes it
# under: an RF2 archive, a FHIR resource or package, or Ontoserver's binary
# index. That answers what the add-on cannot know from fixtures (#602): which
# systems the sync can take from the feed and which still need the release
# from the national release centre.
#
# Local only, by design. It needs a personal licensed account, so it refuses to
# run under CI and no workflow calls it. Credentials come from the environment
# the add-on reads (FERROTERM_NTS_USERNAME and FERROTERM_NTS_PASSWORD, or
# FERROTERM_NTS_CLIENT_ID and FERROTERM_NTS_CLIENT_SECRET) and are never
# printed. The listing is written to a temporary file that is removed on exit
# unless --keep names a path; a kept listing is licensed content, so it stays
# out of the repository.
#
# Usage: scripts/live/nts-feed.sh [--base URL] [--keep FILE] [--listing FILE]
#   --base URL     the service (default https://terminologieserver.nl); the
#                  Belgian FPS server has the same shape
#   --keep FILE    write the raw Atom listing to FILE instead of discarding it
#   --listing FILE report on a saved listing instead of fetching one: no
#                  account needed, which is how the report itself is tried
#                  against the vendored public feeds
# Requires: curl, jq, python3.
set -euo pipefail

if [ -n "${CI:-}" ] || [ -n "${GITHUB_ACTIONS:-}" ]; then
  echo "nts-feed: this check reads a licensed feed with a personal account and never runs under CI" >&2
  exit 2
fi

base="https://terminologieserver.nl"
keep=""
saved=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --base) base="${2:?--base needs a URL}"; shift 2 ;;
    --keep) keep="${2:?--keep needs a file}"; shift 2 ;;
    --listing) saved="${2:?--listing needs a file}"; shift 2 ;;
    -h|--help) sed -n '4,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "usage: $0 [--base URL] [--keep FILE] [--listing FILE]" >&2; exit 2 ;;
  esac
done

for tool in curl jq python3; do
  command -v "$tool" >/dev/null 2>&1 || { echo "nts-feed: missing required tool: $tool" >&2; exit 2; }
done

if [ -n "$saved" ]; then
  [ -f "$saved" ] || { echo "nts-feed: no such listing: $saved" >&2; exit 2; }
  listing="$saved"
  echo "== listing: $saved ($(wc -c <"$listing" | tr -d ' ') bytes, saved)"
else
user="${FERROTERM_NTS_USERNAME:-}"
pass="${FERROTERM_NTS_PASSWORD:-}"
client_id="${FERROTERM_NTS_CLIENT_ID:-}"
client_secret="${FERROTERM_NTS_CLIENT_SECRET:-}"
if [ -z "$client_secret" ] && { [ -z "$user" ] || [ -z "$pass" ]; }; then
  echo "nts-feed: set FERROTERM_NTS_USERNAME and FERROTERM_NTS_PASSWORD (or FERROTERM_NTS_CLIENT_ID and FERROTERM_NTS_CLIENT_SECRET)" >&2
  exit 2
fi

echo "== discovery: ${base}/fhir/.well-known/smart-configuration"
token_endpoint="$(curl --proto '=https' --tlsv1.2 -sSfL --max-redirs 3 "${base}/fhir/.well-known/smart-configuration" | jq -r '.token_endpoint')"
[ -n "$token_endpoint" ] && [ "$token_endpoint" != "null" ] || { echo "nts-feed: the discovery document names no token_endpoint" >&2; exit 1; }
echo "   token endpoint: ${token_endpoint}"

if [ -n "$client_secret" ]; then
  echo "== token: client_credentials as ${client_id:-<no client id>}"
  token_json="$(curl --proto '=https' --tlsv1.2 -sSf -X POST "$token_endpoint" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data-urlencode 'grant_type=client_credentials' \
    --data-urlencode "client_id=${client_id}" \
    --data-urlencode "client_secret=${client_secret}")"
else
  echo "== token: password grant as ${user} with client_id cli_client (the documented grant)"
  token_json="$(curl --proto '=https' --tlsv1.2 -sSf -X POST "$token_endpoint" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data-urlencode 'grant_type=password' \
    --data-urlencode 'client_id=cli_client' \
    --data-urlencode "username=${user}" \
    --data-urlencode "password=${pass}")"
fi
access_token="$(jq -r '.access_token // empty' <<<"$token_json")"
[ -n "$access_token" ] || { echo "nts-feed: no access_token in the token response" >&2; exit 1; }
# The lifetimes are what the add-on's refresh logic is built on (24 h claimed
# for the refresh token by the Nictiz FAQ); print them, never the tokens.
echo "   access token lifetime:  $(jq -r '.expires_in // "?"' <<<"$token_json") s"
echo "   refresh token lifetime: $(jq -r '.refresh_expires_in // "?"' <<<"$token_json") s"

listing="${keep:-$(mktemp)}"
if [ -z "$keep" ]; then
  trap 'rm -f "$listing"' EXIT
fi
echo "== listing: ${base}/synd/syndication.xml"
curl --proto '=https' --tlsv1.2 -sSf -H "Authorization: Bearer ${access_token}" \
  -o "$listing" "${base}/synd/syndication.xml"
echo "   $(wc -c <"$listing" | tr -d ' ') bytes${keep:+, kept at $keep}"
fi

python3 - "$listing" <<'PY'
import sys, xml.etree.ElementTree as ET
from collections import Counter, defaultdict
ns = {"a": "http://www.w3.org/2005/Atom",
      "ncts": "http://ns.electronichealth.net.au/ncts/syndication/asf/extensions/1.0.0",
      "sct": "http://snomed.info/syndication/sct-extension/1.0.0"}
root = ET.parse(sys.argv[1]).getroot()
entries = root.findall("a:entry", ns)
print(f"== {len(entries)} entries")
by_term = Counter()
by_system = defaultdict(set)
rows = []
for e in entries:
    title = (e.findtext("a:title", default="", namespaces=ns) or "").strip()
    terms = [c.get("term", "") for c in e.findall("a:category", ns)]
    ident = e.findtext("ncts:contentItemIdentifier", default="", namespaces=ns) or ""
    version = e.findtext("ncts:contentItemVersion", default="", namespaces=ns) or ""
    edition = e.findtext("sct:edition", default="", namespaces=ns) or ""
    updated = (e.findtext("a:updated", default="", namespaces=ns) or "")[:10]
    for t in terms:
        by_term[t] += 1
        by_system[ident or edition or title].add(t)
    rows.append((updated, ",".join(terms), ident or edition, version, title[:60]))
rows.sort(reverse=True)
print(f"{'updated':10} {'category':28} {'identifier':52} {'version':12} title")
for r in rows:
    print(f"{r[0]:10} {r[1]:28} {r[2]:52} {r[3]:12} {r[4]}")
print("\n== entries per category term")
for term, n in by_term.most_common():
    print(f"{n:5}  {term}")
rf2 = {s for s, ts in by_system.items() if any(t.startswith("SCT_RF2") for t in ts)}
fhir = {s for s, ts in by_system.items() if any(t.startswith("FHIR_") for t in ts)}
binary = {s for s, ts in by_system.items() if any("BINARY" in t.upper() or "INDEX" in t.upper() for t in ts)}
print("\n== what the sync can take")
print(f"   RF2 archives (built by ferroterm-build): {len(rf2)} system(s)")
print(f"   FHIR resources or packages (managed directory): {len(fhir)} system(s)")
only_binary = binary - rf2 - fhir
print(f"   binary index only (not syndicable, bring the release yourself): {len(only_binary)} system(s)")
for s in sorted(only_binary):
    print(f"      {s}")
PY
echo "ok: nts-feed (nothing was written to the repository)"
