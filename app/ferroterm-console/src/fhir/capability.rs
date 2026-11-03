// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the shell and the Server screen read of a root's
//! `CapabilityStatement`, decoded at the release that served it.

use crate::fhir::release::Reads;

/// The canonical a root declares an operation's invocation levels under.
///
/// A `CapabilityStatement` records that a server answers an operation and
/// never at which level; `OperationDefinition.system`, `.type`, and
/// `.instance` carry that
/// (<https://hl7.org/fhir/R4B/operationdefinition-definitions.html#OperationDefinition.instance>).
/// FerroTERM states the levels its generated definition declares as an
/// extension, and a root that declares none reads as an operation with no
/// stated level.
const OPERATION_LEVEL: &str = "https://ferroterm.eu/fhir/StructureDefinition/operation-level";

/// What `GET [base]/metadata` tells the viewer about a served root.
///
/// The FHIR RESTful API defines `metadata` as the capabilities interaction
/// (<https://hl7.org/fhir/R4B/http.html#capabilities>); the statement
/// decodes through `fhir-types` at its release, and this is what the
/// screens draw of it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Capabilities {
    /// `CapabilityStatement.fhirVersion`, the release this root speaks.
    pub(crate) fhir_version: Option<String>,
    /// `CapabilityStatement.software`, which names the running server.
    pub(crate) software: Option<Software>,
    /// `CapabilityStatement.description`, where a root states what it serves.
    pub(crate) description: Option<String>,
    /// Every operation the root declares, the whole-root ones first, in the
    /// document's order.
    declared: Vec<OperationDeclaration>,
    /// Each `rest.resource`, with what it declares.
    resources: Vec<Served>,
}

/// One `CapabilityStatement.rest.resource`, as the editors read it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Served {
    /// `resource.type`.
    resource_type: String,
    /// The `resource.interaction` codes.
    interactions: Vec<String>,
    /// The `resource.operation` names, as the statement wrote them.
    operations: Vec<String>,
}

/// One operation a root declares, as the statement wrote it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct OperationDeclaration {
    /// The resource type it is declared on, empty for a whole-root operation.
    pub(crate) resource: String,
    /// The operation code, without the `$` a URL spells it with.
    pub(crate) code: String,
    /// The `OperationDefinition` canonical the statement names.
    pub(crate) definition: Option<String>,
    /// What the root says it answers beyond that definition.
    pub(crate) documentation: Option<String>,
    /// The invocation levels the root declares, in the order it wrote them.
    pub(crate) levels: Vec<String>,
}

/// `CapabilityStatement.software`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Software {
    /// The software's own name.
    pub(crate) name: Option<String>,
    /// The version of the running software.
    pub(crate) version: Option<String>,
}

impl Capabilities {
    /// The name of the running software, with its version where it gave one.
    pub(crate) fn software_line(&self) -> Option<String> {
        let software = self.software.as_ref()?;
        match (software.name.as_deref(), software.version.as_deref()) {
            (Some(name), Some(version)) => Some(format!("{name} {version}")),
            (Some(name), None) => Some(name.to_owned()),
            (None, Some(version)) => Some(version.to_owned()),
            (None, None) => None,
        }
    }

    /// Every operation this root declares, the whole-root ones first.
    ///
    /// The order is the document's own, so a screen that draws them shows what
    /// the root sent rather than an order of its own.
    pub(crate) fn declarations(&self) -> Vec<OperationDeclaration> {
        self.declared.clone()
    }

    /// Whether this root declares `operation` on `resource_type`.
    ///
    /// An affordance that runs an operation appears only where the statement
    /// declares it, so a screen never offers a run the root then refuses. A
    /// leading `$` is trimmed from both sides of the comparison: R4B's own
    /// example instances write the bare name
    /// (<https://hl7.org/fhir/R4B/capabilitystatement-example.json.html>) and
    /// a statement that writes `$translate` means the same operation.
    pub(crate) fn declares_operation(&self, resource_type: &str, operation: &str) -> bool {
        let wanted = operation.trim_start_matches('$');
        self.served(resource_type)
            .flat_map(|served| served.operations.iter())
            .any(|name| name.trim_start_matches('$') == wanted)
    }

    /// Whether this root declares `interaction` on `resource_type`.
    ///
    /// The codes are the type-restful-interaction value set's: `read`,
    /// `vread`, `update`, `history-instance`, and the rest
    /// (<https://hl7.org/fhir/R4B/capabilitystatement.html>). A screen offers
    /// an affordance only where the statement declares the interaction behind
    /// it, so a root that counts versions one way and a root that counts them
    /// another are both read for what they say they answer.
    pub(crate) fn declares_interaction(&self, resource_type: &str, interaction: &str) -> bool {
        self.served(resource_type)
            .flat_map(|served| served.interactions.iter())
            .any(|code| code == interaction)
    }

    /// The `rest.resource` entries declared for `resource_type`.
    fn served<'a>(&'a self, resource_type: &'a str) -> impl Iterator<Item = &'a Served> + 'a {
        self.resources
            .iter()
            .filter(move |served| served.resource_type == resource_type)
    }
}

/// The code an operation is compared by: its name without the `$` a URL
/// spells it with. R4B's own example instances write the bare name
/// (<https://hl7.org/fhir/R4B/capabilitystatement-example.json.html>) and a
/// statement that writes `$translate` names the same operation.
fn code_of(name: Option<&str>) -> String {
    name.unwrap_or_default().trim_start_matches('$').to_owned()
}

/// Reads the `CapabilityStatement` of the `fhir-types` module `$module`,
/// whose shape is the same at every release the server mounts.
macro_rules! reads_capabilities {
    ($module:ident) => {
        impl Reads<Capabilities> for fhir_types::$module::capability_statement::CapabilityStatement {
            fn read(&self) -> Capabilities {
                use fhir_types::$module::capability_statement::CapabilityStatementRestResourceOperation;
                use fhir_types::$module::extension::ExtensionValue;
                let declaration = |operation: &CapabilityStatementRestResourceOperation, resource: &str| {
                    OperationDeclaration {
                        resource: resource.to_owned(),
                        code: code_of(operation.name.value.as_deref()),
                        definition: operation.definition.value.clone(),
                        documentation: operation
                            .documentation
                            .as_ref()
                            .and_then(|text| text.value.clone()),
                        levels: operation
                            .extension
                            .iter()
                            .filter(|extension| extension.url == OPERATION_LEVEL)
                            .filter_map(|extension| match &extension.value {
                                Some(ExtensionValue::Code(code)) => code.value.clone(),
                                _ => None,
                            })
                            .collect(),
                    }
                };
                let mut declared = Vec::new();
                let mut resources = Vec::new();
                for rest in &self.rest {
                    declared.extend(rest.operation.iter().map(|operation| declaration(operation, "")));
                    for resource in &rest.resource {
                        let of = resource.r#type.value.clone().unwrap_or_default();
                        declared.extend(resource.operation.iter().map(|operation| declaration(operation, &of)));
                        resources.push(Served {
                            interactions: resource
                                .interaction
                                .iter()
                                .filter_map(|interaction| interaction.code.value.clone())
                                .collect(),
                            operations: resource
                                .operation
                                .iter()
                                .filter_map(|operation| operation.name.value.clone())
                                .collect(),
                            resource_type: of,
                        });
                    }
                }
                Capabilities {
                    fhir_version: self.fhir_version.value.clone(),
                    software: self.software.as_ref().map(|software| Software {
                        name: software.name.value.clone(),
                        version: software.version.as_ref().and_then(|version| version.value.clone()),
                    }),
                    description: self.description.as_ref().and_then(|text| text.value.clone()),
                    declared,
                    resources,
                }
            }
        }
    };
}

reads_capabilities!(r4);
reads_capabilities!(r4b);
reads_capabilities!(r5);
reads_capabilities!(r6);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::fhir::version::FhirVersion;

    /// An otherwise minimal R4B statement whose `rest` is `rest`, or one
    /// with no `rest` for the empty string.
    fn parse(rest: &str) -> Capabilities {
        let rest = if rest.is_empty() {
            String::new()
        } else {
            format!(r#","rest":{rest}"#)
        };
        let json = format!(
            r#"{{"resourceType":"CapabilityStatement","status":"active","date":"2026-01-01",
                "kind":"instance","fhirVersion":"4.3.0","format":["json"]{rest}}}"#
        );
        FhirVersion::R4B
            .decode(&json)
            .expect("the fixture is a valid R4B statement")
    }

    #[test]
    fn an_interaction_is_declared_only_where_the_statement_declares_it() {
        let statement = parse(
            r#"[{"mode":"server","resource":[
                 {"type":"CodeSystem","interaction":[{"code":"read"},{"code":"vread"}]},
                 {"type":"ValueSet","interaction":[{"code":"read"}]}]}]"#,
        );
        assert!(statement.declares_interaction("CodeSystem", "vread"));
        assert!(
            !statement.declares_interaction("CodeSystem", "history-instance"),
            "a root that does not declare the history interaction is not asked for it"
        );
        assert!(
            !statement.declares_interaction("ValueSet", "vread"),
            "an interaction is declared per resource type"
        );
        assert!(!statement.declares_interaction("ConceptMap", "read"));
    }

    #[test]
    fn an_operation_is_declared_only_where_the_statement_names_it() {
        let statement = parse(
            r#"[{"mode":"server","resource":[
                {"type":"ConceptMap","operation":[{"name":"translate","definition":"http://hl7.org/fhir/OperationDefinition/x"}]},
                {"type":"ValueSet","operation":[{"name":"expand","definition":"http://hl7.org/fhir/OperationDefinition/x"}]}]}]"#,
        );
        assert!(statement.declares_operation("ConceptMap", "translate"));
        assert!(
            statement.declares_operation("ConceptMap", "$translate"),
            "a statement that writes the bare name means the same operation"
        );
        assert!(
            !statement.declares_operation("ValueSet", "translate"),
            "an operation declared on one type is not declared on another"
        );
        assert!(!statement.declares_operation("CodeSystem", "lookup"));
    }

    #[test]
    fn a_statement_that_declares_no_rest_offers_no_operation() {
        assert!(
            !parse("").declares_operation("ConceptMap", "translate"),
            "the screen then says the root does not declare the operation"
        );
    }

    #[test]
    fn a_declaration_carries_the_levels_and_the_note_the_root_wrote() {
        let statement = parse(
            r#"[{"mode":"server","resource":[
                {"type":"CodeSystem","operation":[{"name":"lookup",
                  "definition":"http://hl7.org/fhir/OperationDefinition/CodeSystem-lookup",
                  "documentation":"the root says what it adds",
                  "extension":[
                    {"url":"https://ferroterm.eu/fhir/StructureDefinition/operation-level","valueCode":"type"},
                    {"url":"https://example.org/other","valueCode":"ignored"},
                    {"url":"https://ferroterm.eu/fhir/StructureDefinition/operation-level","valueCode":"instance"}]}]}]}]"#,
        );
        let declared = statement.declarations();
        assert_eq!(declared.len(), 1, "{declared:?}");
        let lookup = &declared[0];
        assert_eq!(lookup.resource, "CodeSystem");
        assert_eq!(lookup.code, "lookup");
        assert_eq!(lookup.levels, ["type", "instance"]);
        assert_eq!(
            lookup.definition.as_deref(),
            Some("http://hl7.org/fhir/OperationDefinition/CodeSystem-lookup")
        );
        assert_eq!(
            lookup.documentation.as_deref(),
            Some("the root says what it adds")
        );
    }

    #[test]
    fn an_operation_the_root_answers_on_itself_names_no_resource_type() {
        let statement = parse(
            r#"[{"mode":"server",
                "operation":[{"name":"$closure","definition":"http://hl7.org/fhir/OperationDefinition/x"}],
                "resource":[{"type":"ValueSet","operation":[{"name":"expand","definition":"http://hl7.org/fhir/OperationDefinition/x"}]}]}]"#,
        );
        let declared = statement.declarations();
        let named: Vec<(&str, &str)> = declared
            .iter()
            .map(|operation| (operation.resource.as_str(), operation.code.as_str()))
            .collect();
        assert_eq!(
            named,
            [("", "closure"), ("ValueSet", "expand")],
            "a whole-root operation comes first, and the `$` is not part of the code"
        );
        assert!(
            declared[0].levels.is_empty(),
            "a root that states no level has none read into it"
        );
    }
}

#[cfg(test)]
mod strict {
    use crate::fhir::capability::Capabilities;
    use crate::fhir::version::FhirVersion;

    #[test]
    fn a_statement_that_lacks_a_required_element_is_refused_by_its_path() {
        let error = FhirVersion::R4
            .decode::<Capabilities>(
                r#"{"resourceType":"CapabilityStatement","fhirVersion":"4.0.1"}"#,
            )
            .expect_err("status, date, kind and format are required");
        assert!(error.to_string().contains("CapabilityStatement"), "{error}");
    }

    #[test]
    fn a_member_the_release_does_not_define_is_refused() {
        let error = FhirVersion::R4
            .decode::<Capabilities>(
                r#"{"resourceType":"CapabilityStatement","status":"active","date":"2026-01-01",
                    "kind":"instance","fhirVersion":"4.0.1","format":["json"],"x-extra":[1]}"#,
            )
            .expect_err("an unknown member is a defect");
        assert!(error.to_string().contains("x-extra"), "{error}");
    }
}
