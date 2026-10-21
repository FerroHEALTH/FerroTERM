//! What this server adds to the SMART discovery document `ferrohealth-smart`
//! builds from the issuer's metadata.
//!
//! The document is this server's own assertion about itself
//! (<https://hl7.org/fhir/smart-app-launch/conformance.html>). The crate keeps
//! every member to the values the specification defines and writes `issuer`
//! only where `sso-openid-connect` is claimed; this module declares the
//! permission capabilities the write routes enforce, and never
//! `permission-patient`, because a `patient/` scope writes nothing on this
//! server.

use std::collections::BTreeSet;

use ferrohealth_smart::resource::configuration::{
    OpenIdConnect, PermissionCapability, SmartOptions,
};

/// The discovery options of the deployment: the scopes `[auth]
/// scopes_supported` lists and the capabilities `[auth] capabilities` adds.
///
/// The permission capabilities are the v1 scope syntax and `user/` scopes;
/// `sso-openid-connect` is claimed where the issuer lists the `openid` scope
/// (<https://openid.net/specs/openid-connect-discovery-1_0.html>, §3).
#[must_use]
pub fn options(scopes_supported: &[String], capabilities: &[String]) -> SmartOptions {
    // NOTE: `permission-v2` is exemplified only by the search-parameter
    // syntax this server refuses, so withholding it is our own reading
    // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    SmartOptions {
        scopes_supported: scopes_supported.to_vec(),
        capabilities: capabilities.to_vec(),
        permissions: BTreeSet::from([PermissionCapability::V1, PermissionCapability::User]),
        openid_connect: OpenIdConnect::FromIssuer,
        ..SmartOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_smart::resource::configuration::PermissionCapability;

    use super::options;

    // A terminology server holds no patient record, so a `patient/` scope
    // writes nothing here and the capability that promises them is withheld.
    #[test]
    fn permission_patient_is_never_claimed() {
        let declared = options(&[], &[]).permissions;
        assert!(
            !declared.contains(&PermissionCapability::Patient),
            "{declared:?}"
        );
        assert!(
            !declared.contains(&PermissionCapability::V2),
            "{declared:?}"
        );
        assert!(declared.contains(&PermissionCapability::V1));
        assert!(declared.contains(&PermissionCapability::User));
    }

    // Additional capabilities are listed as `[auth] capabilities` names them
    // (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    #[test]
    fn a_configured_capability_is_carried() {
        let added = [String::from("launch-ehr")];
        assert_eq!(options(&[], &added).capabilities, added);
    }

    // The server SHALL support every scope it lists, so the list is the
    // configured one (<https://hl7.org/fhir/smart-app-launch/conformance.html>).
    #[test]
    fn scopes_supported_is_the_configured_list_alone() {
        assert_eq!(options(&[], &[]).scopes_supported, Vec::<String>::new());
        let listed = [String::from("system/CodeSystem.cud")];
        assert_eq!(options(&listed, &[]).scopes_supported, listed);
    }
}
