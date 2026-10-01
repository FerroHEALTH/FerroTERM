#!/usr/bin/env bash
# assemble.sh: the site as GitHub Pages serves it, from the landing page and the
# book, into one directory.
#
#   scripts/site/assemble.sh OUT
#
# The landing page is served at the site root and the mdBook under /docs/
# (book.toml sets site-url = "/docs/"). The landing page's roadmap block
# (between `<!-- roadmap:begin -->` and `<!-- roadmap:end -->`) is rendered
# here from the tracker's open milestones through the GitHub API when `gh`
# is authenticated; without it the block stays empty and the page keeps its
# link to the milestones, so a local assembly never fails on the network.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly OUT="${1:?usage: $0 OUT}"
readonly LANDING=website/landing
readonly REPO=FerroHEALTH/FerroTERM

roadmap() {
  gh api "repos/$REPO/milestones?state=open&sort=due_on&direction=asc" --jq '
    def esc: gsub("&"; "&amp;") | gsub("<"; "&lt;") | gsub(">"; "&gt;");
    ["<ul class=\"roadmap\">"]
    + map("  <li><span class=\"tag\">" + (.title | esc) + "</span>"
          + "<a href=\"" + .html_url + "\" rel=\"noopener\">" + (.open_issues | tostring) + " open, " + (.closed_issues | tostring) + " closed</a>"
          + (if .due_on == null then "" else "<span class=\"due\">due " + .due_on[0:10] + "</span>" end)
          + "</li>")
    + ["</ul>"] | .[]'
}

mdbook build website/book
rm -rf "$OUT"
mkdir -p "$OUT/docs"
cp -R "$LANDING"/. "$OUT/"
cp -R website/book/book/. "$OUT/docs/"

block="$(mktemp)"
trap 'rm -f "$block"' EXIT
if roadmap > "$block" 2>/dev/null && [[ -s "$block" ]]; then
  awk -v blockfile="$block" '
    /<!-- roadmap:begin -->/ { print; while ((getline line < blockfile) > 0) print line; skip = 1; next }
    /<!-- roadmap:end -->/ { skip = 0 }
    !skip { print }
  ' "$LANDING/index.html" > "$OUT/index.html"
  echo "assemble: roadmap rendered from the open milestones."
else
  echo "assemble: no GitHub API access; the roadmap block stays empty." >&2
fi
echo "assemble: site at $OUT"
