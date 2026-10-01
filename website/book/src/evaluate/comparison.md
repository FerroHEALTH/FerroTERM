# How it compares

FerroTERM serves the same FHIR terminology API as the established servers. The
difference is the runtime footprint, the stack it needs to operate, and how
far along the feature set is. This page sets FerroTERM beside Snowstorm,
Ontoserver, and Hermes.

> [!NOTE]
> The numbers for the other servers come from their own documentation and
> published work, cited in
> [`docs/architecture.md`](https://github.com/FerroHEALTH/FerroTERM/blob/main/docs/architecture.md).
> FerroTERM's own figures are measurements over the licensed Dutch SNOMED CT
> edition on a laptop, and each one links the record that produced it, naming
> the release it was measured on; see
> [Benchmarks](benchmarks.md) and [Hardware sizing](../operate/hardware-sizing.md).

## The servers

- **Snowstorm** (SNOMED International): Java on Elasticsearch. The reference
  server and FerroTERM's behavioural oracle. Full-featured, and a full
  International edition deployment wants 16 to 32 GB of RAM plus a search
  cluster.
- **Snowstorm Lite** (SNOMED International): drops Elasticsearch for a single
  Lucene index and runs the International edition in about 500 MB.
- **Ontoserver** (CSIRO): Postgres plus Lucene. A production FHIR terminology
  server, index-materialized rather than a graph database; the Dutch national
  terminology server runs it.
- **Hermes** (Mark Wardle): Clojure, a memory-mapped store plus Lucene. It
  serves subsumption in tens of microseconds from a materialized structure,
  the design point FerroTERM follows in pure Rust.

## Side by side

| Server | Language / runtime | Store | Edition footprint | Deploys as |
|---|---|---|---|---|
| Snowstorm | Java (JVM) | Elasticsearch | 16 to 32 GB RAM | JVM plus a search cluster |
| Snowstorm Lite | Java (JVM) | Lucene | about 500 MB | JVM service |
| Ontoserver | Java (JVM) | Postgres plus Lucene | server plus database | JVM plus Postgres |
| Hermes | Clojure (JVM) | memory-mapped store plus Lucene | modest | JVM service |
| FerroTERM 0.1.6 | Rust | `redb` plus roaring and `fst` artifacts, read at startup | 559 to 864 MB on disk, 943 MB to 1.14 GB resident, per SNOMED edition | one binary (statically linked on the musl targets) or a distroless image |

## What FerroTERM trades

FerroTERM is younger. Hierarchical expansion, persisted client resources,
`Bundle` batch, `$closure`, the SNOMED implicit value sets and implicit concept
maps, ECL, the R4, R4B, R5, and R6 endpoints, and XML are served; the persisted
resources and the closure tables need a deployment that names a database in
`FERROTERM_RESOURCES`. FerroTERM serves an index built from a finished release,
so it authors no SNOMED content and has no syndication client to download one,
and MRCM validation of postcoordinated expressions is unscheduled. What
it serves, it serves from one binary with signed provenance, over more code
systems than the SNOMED-only servers, and with its conformance measured by the
HL7 terminology ecosystem suite on every change.

Every production server in this list converges on a materialized index rather
than live graph traversal. FerroTERM takes the same shape in pure Rust, with a
machine-generated FHIR layer across four versions.
