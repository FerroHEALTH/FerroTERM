- `$expand` echoes the extensions the terminology ecosystem guide lists on
  each `expansion.contains` entry, where the code system or the value set
  states them on the concept: `valueset-label` (from `valueset-label`, else
  `codesystem-label`), `valueset-conceptOrder` (likewise from
  `codesystem-conceptOrder`) and `rendering-style` on the entry, and
  `rendering-xhtml` on its `display`. A value set's own statement wins over
  its code system's. `valueset-concept-definition` is echoed when the
  request asks for the `definition` property. `itemWeight` and
  `valueset-deprecated` are not echoed, because their published contexts
  (`Coding`, `ValueSet.compose.include.concept`) admit no expansion
  element. The crate line moves to 0.1.142.
- An inactive designation in an expansion, one whose
  `structuredefinition-standards-status` is `withdrawn` or `deprecated`,
  carries the SNOMED CT use code `900000000000546006` (Inactive value), as
  the terminology ecosystem guide asks. That extension's context does not
  admit `contains.designation`. A designation that already states a use
  cannot carry both, so it is left out of the expansion.
