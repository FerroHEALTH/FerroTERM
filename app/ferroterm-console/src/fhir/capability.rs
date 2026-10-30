// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The part of a `CapabilityStatement` the shell renders.

use serde::{Deserialize, Serialize};

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
/// (<https://hl7.org/fhir/R4B/http.html#capabilities>), and the viewer reads
/// only the elements it draws. Every field is optional here so a server that
/// omits one still renders.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct CapabilityStatement {
    /// `CapabilityStatement.fhirVersion`, the release this root speaks.
    #[serde(rename = "fhirVersion")]
    pub(crate) fhir_version: Option<String>,
    /// `CapabilityStatement.software`, which names the running server.
    pub(crate) software: Option<Software>,
    /// `CapabilityStatement.description`, where a root states what it serves.
    pub(crate) description: Option<String>,
    /// `CapabilityStatement.rest`, which says what each resource type answers.
    #[serde(default)]
    rest: Vec<Rest>,
}

/// One `CapabilityStatement.rest`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Rest {
    /// `rest.resource`, one per resource type this root serves.
    #[serde(default)]
    resource: Vec<RestResource>,
    /// `rest.operation`, the operations this root answers at its own root.
    #[serde(default)]
    operation: Vec<RestOperation>,
    /// `rest.security`, where a root names the authentication in front of it.
    security: Option<Security>,
}

/// `CapabilityStatement.rest.security`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Security {
    /// `security.service`, as codes of the FHIR `restful-security-service`
    /// value set (<https://hl7.org/fhir/R4B/valueset-restful-security-service.html>).
    #[serde(default)]
    service: Vec<SecurityService>,
}

/// One `security.service`, read for the codes it carries.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct SecurityService {
    /// The codings that name the service.
    #[serde(default)]
    coding: Vec<ServiceCoding>,
}

/// One coding of `security.service`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct ServiceCoding {
    /// The code itself.
    code: Option<String>,
}

/// One `CapabilityStatement.rest.resource`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct RestResource {
    /// `resource.type`, the resource type it describes.
    r#type: Option<String>,
    /// `resource.interaction`, the RESTful interactions this root answers.
    ///
    /// Only the editors read it, to offer a write the server answers.
    #[serde(default)]
    interaction: Vec<RestInteraction>,
    /// `resource.operation`, the operations this root answers on that type.
    #[serde(default)]
    operation: Vec<RestOperation>,
}

/// One `CapabilityStatement.rest.resource.interaction`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct RestInteraction {
    /// `interaction.code`, a code of the type-restful-interaction value set
    /// (<https://hl7.org/fhir/R4B/capabilitystatement.html>).
    code: Option<String>,
}

/// One `CapabilityStatement.rest.resource.operation`.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct RestOperation {
    /// `operation.name`, as the statement wrote it.
    name: Option<String>,
    /// `operation.definition`, the `OperationDefinition` it points at.
    definition: Option<String>,
    /// `operation.documentation`, what this root adds beyond that definition.
    documentation: Option<String>,
    /// The extensions the root declared on the operation.
    #[serde(default)]
    extension: Vec<Extension>,
}

/// One `Extension`, read only for the invocation levels it may carry.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Extension {
    /// The canonical that says what the extension means.
    url: Option<String>,
    /// `valueCode`, the one value type read here.
    #[serde(rename = "valueCode")]
    value_code: Option<String>,
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
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
pub(crate) struct Software {
    /// The software's own name.
    pub(crate) name: Option<String>,
    /// The version of the running software.
    pub(crate) version: Option<String>,
}

impl CapabilityStatement {
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
        let mut declared = Vec::new();
        for rest in &self.rest {
            declared.extend(
                rest.operation
                    .iter()
                    .map(|operation| operation.read(String::new())),
            );
            for resource in &rest.resource {
                let of = resource.r#type.clone().unwrap_or_default();
                declared.extend(
                    resource
                        .operation
                        .iter()
                        .map(|operation| operation.read(of.clone())),
                );
            }
        }
        declared
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
        self.rest
            .iter()
            .flat_map(|rest| rest.resource.iter())
            .filter(|resource| resource.r#type.as_deref() == Some(resource_type))
            .flat_map(|resource| resource.operation.iter())
            .filter_map(|declared| declared.name.as_deref())
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
        self.rest
            .iter()
            .flat_map(|rest| rest.resource.iter())
            .filter(|resource| resource.r#type.as_deref() == Some(resource_type))
            .flat_map(|resource| resource.interaction.iter())
            .filter_map(|declared| declared.code.as_deref())
            .any(|code| code == interaction)
    }
}

impl RestOperation {
    /// This operation as the comparison reads it, declared on `resource`.
    ///
    /// The `$` is trimmed from the code: R4B's own example instances write the
    /// bare name (<https://hl7.org/fhir/R4B/capabilitystatement-example.json.html>)
    /// and a statement that writes `$translate` names the same operation.
    fn read(&self, resource: String) -> OperationDeclaration {
        OperationDeclaration {
            resource,
            code: self
                .name
                .as_deref()
                .unwrap_or_default()
                .trim_start_matches('$')
                .to_owned(),
            definition: self.definition.clone(),
            documentation: self.documentation.clone(),
            levels: self
                .extension
                .iter()
                .filter(|extension| extension.url.as_deref() == Some(OPERATION_LEVEL))
                .filter_map(|extension| extension.value_code.clone())
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> CapabilityStatement {
        serde_json::from_str(json).expect("the fixture is valid JSON")
    }

    #[test]
    fn an_interaction_is_declared_only_where_the_statement_declares_it() {
        let statement = parse(
            r#"{"resourceType":"CapabilityStatement","rest":[{"resource":[
                 {"type":"CodeSystem","interaction":[{"code":"read"},{"code":"vread"}]},
                 {"type":"ValueSet","interaction":[{"code":"read"}]}]}]}"#,
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
            r#"{"resourceType":"CapabilityStatement","rest":[{"mode":"server","resource":[
                {"type":"ConceptMap","operation":[{"name":"translate"}]},
                {"type":"ValueSet","operation":[{"name":"expand"}]}]}]}"#,
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
            !parse(r#"{"fhirVersion":"4.3.0"}"#).declares_operation("ConceptMap", "translate"),
            "the screen then says the root does not declare the operation"
        );
    }

    #[test]
    fn a_declaration_carries_the_levels_and_the_note_the_root_wrote() {
        let statement = parse(
            r#"{"resourceType":"CapabilityStatement","rest":[{"mode":"server","resource":[
                {"type":"CodeSystem","operation":[{"name":"lookup",
                  "definition":"http://hl7.org/fhir/OperationDefinition/CodeSystem-lookup",
                  "documentation":"the root says what it adds",
                  "extension":[
                    {"url":"https://ferroterm.eu/fhir/StructureDefinition/operation-level","valueCode":"type"},
                    {"url":"https://example.org/other","valueCode":"ignored"},
                    {"url":"https://ferroterm.eu/fhir/StructureDefinition/operation-level","valueCode":"instance"}]}]}]}]}"#,
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
            r#"{"resourceType":"CapabilityStatement","rest":[{"mode":"server",
                "operation":[{"name":"$closure"}],
                "resource":[{"type":"ValueSet","operation":[{"name":"expand"}]}]}]}"#,
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
