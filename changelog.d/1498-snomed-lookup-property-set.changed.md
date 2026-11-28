- SNOMED CT `$lookup` follows the terminology ecosystem IG and the HL7
  Terminology SNOMED CT page: `property=*` returns every property except
  `normalForm` and `normalFormTerse`, which answer only when named; the new
  `semanticTag` property is the phrase in the last brackets of the fully
  specified name; `effectiveTime` is a `dateTime` (`2005-01-31`) where it was
  the RF2 string `20050131`. With no property named, `sufficientlyDefined`
  and `semanticTag` are left out (#1498).
