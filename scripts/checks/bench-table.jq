# The public speed and footprint figures from one set of ferroterm-bench
# records (jq -n: every input file is one record, named by input_filename).
# $target selects the output: "readme" (the README's markdown table),
# "figures" (the landing page's figure tiles, HTML), or "benchmarks" (the
# benchmarks page's full table, HTML). Units fit the value: seconds,
# milliseconds, or microseconds; GB, MB, or KB.
def duration(ms):
  if ms == null then "n/a"
  elif ms >= 1000 then ((ms / 10 | round) / 100 | tostring) + " s"
  elif ms >= 1 then ((ms * 100 | round) / 100 | tostring) + " ms"
  else ((ms * 1000 | round) | tostring) + " µs"
  end;
def bytes(b):
  if b == null then "n/a"
  elif b >= 1000000000 then ((b / 10000000 | round) / 100 | tostring) + " GB"
  elif b >= 1000000 then ((b / 1000000) | round | tostring) + " MB"
  else ((b / 1000) | round | tostring) + " KB"
  end;
def count(n): (n // 0) | tostring | [match("\\d{1,3}(?=(\\d{3})*$)"; "g").string] | join(",");
def op(r; name): (r.latency[] | select(.[0] == name) | .[1]) // null;
def p50(r; name): (op(r; name)) as $o | if $o == null then "n/a" else duration($o.p50_ms) end;
def build(r): if r.ingest == null then "n/a" else duration(r.ingest.seconds * 1000) end;
def build_rss(r): if r.ingest == null then "n/a" else bytes(r.ingest.peak_rss_bytes) end;
# A SNOMED CT version is its edition URI; the release date after "/version/" is
# what a reader wants in a column.
def release(v): if v == null then "n/a" elif (v | contains("/version/")) then (v | split("/version/")[1]) else v end;
def same(key): (map(key) | unique | length) == 1;
def html(s): s | gsub("&"; "&amp;") | gsub("<"; "&lt;") | gsub(">"; "&gt;");
def record_url(r): "https://github.com/FerroHEALTH/FerroTERM/blob/main/" + r.record;

def conditions($first):
  "Warm p50 over " + ($first.latency[0][1].warm_requests | tostring)
  + " HTTP round trips on one machine"
  + (if $first.machine.container then ", inside a Docker container" else "" end)
  + " (" + $first.machine.cpu + ", " + bytes($first.machine.memory_bytes) + ", "
  + $first.machine.os + "/" + $first.machine.arch + "), FerroTERM " + $first.ferroterm_version
  + " serving FHIR " + ($first.fhir | ascii_upcase) + ", taken " + ($first.taken_at | .[0:10]) + ".";

def readme($rows; $first; link):
  [
    "| Code system | Release | Concepts | Build | Peak build memory | Index on disk | Resident | `$lookup` | `$validate-code` | `$subsumes` | `$expand` (small) | `$expand` (large) | Search | Snowstorm |",
    "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"
  ]
  + ($rows | map(
      "| [" + .system + "](" + link + ")"
      + " | " + release(.system_version)
      + " | " + count(.concepts)
      + " | " + build(.)
      + " | " + build_rss(.)
      + " | " + bytes(.artifact_bytes)
      + " | " + bytes(.rss_warm_bytes)
      + " | " + p50(.; "lookup")
      + " | " + p50(.; "validate-code")
      + " | " + p50(.; "subsumes")
      + " | " + p50(.; "expand-small")
      + " | " + p50(.; "expand-large")
      + " | " + p50(.; "search")
      + " | " + (.comparison // "not run")
      + " |"))
  + [
    "",
    conditions($first) + " The records are under `bench/records/`; the [benchmarks page](https://ferroterm.eu/benchmarks.html) has the method, the cold and tail latencies, and how to reproduce a record."
  ];

# One tile per system on the landing page: what the system costs to run and
# how fast a lookup answers, each linked to its record.
def figures($rows; $first):
  ["<div class=\"figures\">"]
  + ($rows | map(
      "  <a class=\"figure\" href=\"" + record_url(.) + "\" rel=\"noopener\">"
      + "<span class=\"figure-name\">" + html(.system) + "</span>"
      + "<span class=\"figure-big\">" + p50(.; "lookup") + "</span>"
      + "<span class=\"figure-cap\">a <code>$lookup</code>, warm p50 · " + bytes(.rss_warm_bytes) + " resident · " + bytes(.artifact_bytes) + " on disk"
      + (if .ingest == null then "" else " · built in " + build(.) end)
      + "</span></a>"))
  + ["</div>", "<p class=\"figures-note\">" + html(conditions($first)) + " Every tile links to its record.</p>"];

def cell4($o):
  if $o == null then "<td class=\"na\" colspan=\"4\">n/a</td>"
  else "<td>" + duration($o.cold_ms) + "</td><td>" + duration($o.p50_ms) + "</td><td>" + duration($o.p95_ms) + "</td><td>" + duration($o.p99_ms) + "</td>" end;

# The benchmarks page: footprint per system, then latency per operation with
# the cold request and the warm tail, then the comparison per system.
def benchmarks($rows; $first):
  ["<p class=\"conditions\">" + html(conditions($first)) + "</p>",
   "<h3>Footprint</h3>",
   "<div class=\"table-wrap\"><table>",
   "<thead><tr><th>Code system</th><th>Release</th><th>Concepts</th><th>Build</th><th>Peak build memory</th><th>Index on disk</th><th>Time to ready</th><th>Resident, open</th><th>Resident, warm</th></tr></thead>",
   "<tbody>"]
  + ($rows | map(
      "<tr><th scope=\"row\"><a href=\"" + record_url(.) + "\" rel=\"noopener\">" + html(.system) + "</a></th>"
      + "<td>" + html(release(.system_version)) + "</td>"
      + "<td>" + count(.concepts) + "</td>"
      + "<td>" + build(.) + "</td>"
      + "<td>" + build_rss(.) + "</td>"
      + "<td>" + bytes(.artifact_bytes) + "</td>"
      + "<td>" + duration(.ready_seconds * 1000) + "</td>"
      + "<td>" + bytes(.rss_open_bytes) + "</td>"
      + "<td>" + bytes(.rss_warm_bytes) + "</td></tr>"))
  + ["</tbody></table></div>",
     "<h3>Latency per operation</h3>",
     "<p>Each operation: the first request cold, then the nearest-rank p50, p95, and p99 over the warm requests.</p>"]
  + (["lookup", "validate-code", "subsumes", "expand-small", "expand-large", "search"] | map(
      . as $name |
      ["<h4><code>" + ({"lookup": "$lookup", "validate-code": "$validate-code", "subsumes": "$subsumes", "expand-small": "$expand (a small value set)", "expand-large": "$expand (the whole system, one page)", "search": "$expand with filter (designation search)"}[$name]) + "</code></h4>",
       "<div class=\"table-wrap\"><table>",
       "<thead><tr><th>Code system</th><th>Cold</th><th>p50</th><th>p95</th><th>p99</th></tr></thead>",
       "<tbody>"]
      + ($rows | map("<tr><th scope=\"row\">" + html(.system) + "</th>" + cell4(op(.; $name)) + "</tr>"))
      + ["</tbody></table></div>"]) | flatten)
  + ["<h3>Reference-server comparison</h3>",
     "<p>A comparison is stated only when the reference server ran on the same machine over the same release, with its configuration recorded in the record.</p>",
     "<ul>"]
  + ($rows | map("<li><strong>" + html(.system) + ":</strong> " + html(.comparison // "not run") + "</li>"))
  + ["</ul>"];

[inputs | . + {record: input_filename}] |
if (same(.machine) and same(.ferroterm_version) and same(.fhir)) | not then
  error("the records come from more than one machine, FerroTERM version, or FHIR version")
else
  (group_by(.system) | map(max_by(.taken_at)) | sort_by(.system)) as $rows |
  ($rows[0]) as $first |
  (if $target == "readme" then readme($rows; $first; .record)
   elif $target == "book" then readme($rows; $first; record_url(.))
   elif $target == "figures" then figures($rows; $first)
   elif $target == "benchmarks" then benchmarks($rows; $first)
   else error("unknown target " + $target) end) | .[]
end
