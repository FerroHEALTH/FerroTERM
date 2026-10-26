// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What FerroTERM puts into the family's boot banner.
//!
//! `ferrohealth_server::build_info::banner` draws its wordmark, its build and
//! the facts under them, printed before the log subscriber exists and only
//! when the console renders for a person.

use ferrohealth_server::build_info::{self, BuildInfo};
use ferrohealth_telemetry::section::Rendering;

/// The wordmark in the `FIGlet` `standard` face, kept as text so the banner
/// needs no font asset and no dependency.
pub const WORDMARK: &str = r"
 _____                   _____ _____ ____  __  __
|  ___|__ _ __ _ __ ___ |_   _| ____|  _ \|  \/  |
| |_ / _ \ '__| '__/ _ \  | | |  _| | |_) | |\/| |
|  _|  __/ |  | | | (_) | | | | |___|  _ <| |  | |
|_|  \___|_|  |_|  \___/  |_| |_____|_| \_\_|  |_|
";

/// The project site the banner names.
pub const SITE: &str = "https://ferroterm.eu";
/// The maintainer the banner names (`MAINTAINERS.md`).
pub const MAINTAINER: &str = "Ruben Talstra";

/// The commit the build stamped, empty when it named none.
const COMMIT: &str = env!("FERROTERM_GIT_SHA");

/// This build: the version and, when the build named one, the commit.
pub const BUILD: BuildInfo = {
    let info = BuildInfo::new("ferroterm", env!("CARGO_PKG_VERSION"));
    if COMMIT.is_empty() {
        info
    } else {
        info.with_commit(COMMIT)
    }
};

/// The banner of `info`, or `None` unless the console renders `pretty`.
#[must_use]
pub fn render(info: &BuildInfo, rendering: Rendering) -> Option<String> {
    let versions = crate::FHIR_BASES
        .map(|base| base.trim_start_matches('/').to_uppercase())
        .join(", ");
    build_info::banner(
        info,
        WORDMARK.trim_start_matches('\n'),
        &[
            ("FHIR", versions),
            ("site", SITE.to_owned()),
            ("maintainer", MAINTAINER.to_owned()),
        ],
        rendering,
    )
}

/// Prints the banner of this build when the console renders `pretty`.
#[expect(
    clippy::print_stdout,
    reason = "the banner is console output and prints before any log subscriber exists"
)]
pub fn print(rendering: Rendering) {
    if let Some(banner) = render(&BUILD, rendering) {
        print!("{banner}");
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_server::build_info::BuildInfo;
    use ferrohealth_telemetry::section::Rendering;

    use super::render;

    #[test]
    fn the_banner_names_the_version_and_fits_a_terminal() {
        let info = BuildInfo::new("ferroterm", "1.2.3").with_commit("1a2b3c4d5e6f");
        let banner = render(&info, Rendering::Pretty).expect("pretty prints the banner");
        assert!(banner.contains("ferroterm 1.2.3 (1a2b3c4)"), "{banner}");
        assert!(banner.contains("R4, R4B, R5, R6"), "{banner}");
        assert!(banner.contains("Ruben Talstra"), "{banner}");
        assert!(banner.contains("https://ferroterm.eu"), "{banner}");
        assert!(banner.lines().all(|line| line.chars().count() <= 100));
        assert_eq!(banner.lines().count(), 9);
        assert!(render(&info, Rendering::Json).is_none(), "json prints none");
    }
}
