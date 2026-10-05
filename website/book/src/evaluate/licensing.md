# Licensing

FerroTERM is source-available under the Business Source License 1.1. The
`LICENSE` file in the repository is the authority; this page states the same
boundary in the order people ask about it, so you can answer "do I need a
commercial licence?" without reading the parameters.

The code systems are a separate question, answered at the bottom of this page.

## Do you need a commercial licence?

| What you are doing | What you need | Why |
|---|---|---|
| Reading, building, modifying, or redistributing the source | Free | The licence grants this without a fee and without asking anyone; only production use is restricted. |
| Development, testing, evaluation, prototyping | Free | None of these is production use, which is the only thing the Additional Use Grant limits. |
| Personal use | Free | Named in the grant's definition of Non-Commercial Purposes. |
| Academic or scientific research, or teaching | Free | Named in the grant's definition of Non-Commercial Purposes. |
| Production use by a non-profit or public body that is not in the course of a business, does not deliver a service for payment, and is not for commercial advantage | Free | The grant's definition of Non-Commercial Purposes, in the licence's own words. |
| Treating patients, or delivering any other service for payment | Commercial licence | The grant ends with "any other production use, including the delivery of health care or any other service for payment, requires a commercial license from the Licensor". |
| Any production use by a vendor or integrator | Commercial licence | A business's production use is not a Non-Commercial Purpose, so the same closing sentence of the grant applies. |
| Offering FerroTERM, or a work derived from it, to third parties as a hosted, managed, or embedded terminology service | Commercial licence | Carve-out (a) of the grant, which the licence defines as a service through which anyone other than you and your affiliates stores, manages, or queries terminology, code system content, or health data. |
| Selling, sublicensing, or otherwise distributing it for a fee, on its own or inside another product | Commercial licence | Carve-out (b) of the grant. |

The last two rows hold whatever else you are. Hosting FerroTERM for third
parties, and distributing it for a fee, need a commercial licence in every
case, including for an organisation the rows above would otherwise leave free.

## What the licence says

Reading, building, modifying, and redistributing the source is free, without a
fee and without asking anyone, and every non-production use is covered:
development, testing, evaluation, and prototyping.

Production use is free for Non-Commercial Purposes, which the licence defines
as personal use, academic or scientific research, teaching, and use by a
non-profit organisation or public body that is not in the course of a business,
does not deliver a service for payment, and is not for commercial advantage.

Any other production use needs a commercial licence from the Licensor,
including the delivery of health care or any other service for payment.

There is no open-core tier. The engine, the server, and the tools are in one
repository under one licence, and nothing is held back to be sold back to you.

## When it becomes Apache 2.0

Each version becomes Apache License 2.0 four years after that version is
published. The clock runs per version, so the four years start at the release
you are looking at rather than at the project.

## The crate that is Apache 2.0 today

`rf2` (the SNOMED CT release file reader) is published to crates.io under
Apache License 2.0 rather than the Business Source License, so any Rust project
can depend on it without a licence conversation. Every other crate published
from this repository carries the Business Source License and states so in its
own README. The FHIR model FerroTERM builds on, `fhir-types`, is published
under Apache 2.0 by the FerroBRIDGE repository.

## Starting a commercial licence

A commercial licence is arranged with Cadasto B.V., the Licensor, which
handles the business side of FerroTERM: write to
[info@cadasto.com](mailto:info@cadasto.com) or use
<https://www.cadasto.com/contact/>. Technical questions go to the maintainer
named in
[MAINTAINERS.md](https://github.com/FerroHEALTH/FerroTERM/blob/main/MAINTAINERS.md).
Companies and care providers building on FerroTERM are wanted here, and the
commercial licence is the normal path for them.

## The code systems are licensed separately

The licence above covers the software. The content of each code system is
licensed by its publisher: SNOMED CT by SNOMED International, LOINC by
Regenstrief, ICD by the WHO, RxNorm by the NLM. The repository ships none of
it, and a deployment brings the release it is licensed for. UCUM and the IANA
and Unicode registries are vendored under their own licences, recorded beside
the data.
