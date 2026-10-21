---
name: icd11-local-deployment
description: How the WHO ICD-API Docker image is configured (include=<release>_<language>, one entry per release) and which release bundles exist; the local cache lives under data/icd11 and the built artifacts under artifacts/icd11
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The ICD-11 source is the WHO ICD-API local deployment: `docker run -d -p
18090:80 -e acceptLicense=true -e saveAnalytics=false -e include=2026-01_en
whoicd/icd-api`. `include` takes one `<release>_<language>` entry per
release; two entries for one release abort the container ("An item with the
same key has already been added"), and the language picks a prebuilt bundle:
`2026-01` had only an English bundle on 2026-09-03 (French asked for 2026-01
aborts with "Couldn't find the version ... _fr"), while `2025-01_fr` loads a
combined `en-fr` bundle. The tx-ecosystem `icd-11` cases pin 2026-01 and two
of them ask for French, so those two cannot pass from the local deployment.

The API is up about 30 s after start; the walk of all three code systems
(37,211 MMS, 1,665 ICF, 71,565 Foundation entities, English) took 181 s
with 8 threads and left a 442 MB cache under `data/icd11/` (gitignored) and
75 MB of artifacts under `artifacts/icd11/{mms,icf,entity}`. The cache makes
the API unnecessary for later builds. Stop and remove the containers when
done; the owner wants Docker clean afterwards (their unrelated
`ferroehr-testkit-pg18` container is not ours to stop).
