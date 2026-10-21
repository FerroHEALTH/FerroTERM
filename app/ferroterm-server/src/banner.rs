// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The boot banner: the wordmark and what this process is, printed before
//! the log subscriber exists and only when the console is for a person.

/// The wordmark in the `FIGlet` `standard` face, kept as text so the banner
/// needs no font asset and no dependency.
const WORDMARK: &str = r"
 _____                   _____ _____ ____  __  __
|  ___|__ _ __ _ __ ___ |_   _| ____|  _ \|  \/  |
| |_ / _ \ '__| '__/ _ \  | | |  _| | |_) | |\/| |
|  _|  __/ |  | | | (_) | | | | |___|  _ <| |  | |
|_|  \___|_|  |_|  \___/  |_| |_____|_| \_\_|  |_|
";

/// The project site printed under the wordmark.
pub const SITE: &str = "https://ferroterm.eu";
/// The maintainer named under the wordmark (`MAINTAINERS.md`).
pub const MAINTAINER: &str = "Ruben Talstra";

/// Renders the banner for `version`.
#[must_use]
pub fn render(version: &str) -> String {
    let wordmark = WORDMARK.trim_start_matches('\n');
    format!(
        "{wordmark}\n  FHIR terminology server for SNOMED CT and other code systems · v{version}\n  Maintained by {MAINTAINER} · {SITE}\n"
    )
}

/// Prints the banner for this build.
#[expect(
    clippy::print_stdout,
    reason = "the banner is console output and prints before any log subscriber exists"
)]
pub fn print() {
    print!("{}", render(env!("CARGO_PKG_VERSION")));
}

#[cfg(test)]
mod tests {
    use super::render;

    #[test]
    fn the_banner_names_the_version_and_fits_a_terminal() {
        let banner = render("1.2.3");
        assert!(banner.contains("v1.2.3"));
        assert!(banner.contains("Maintained by Ruben Talstra · https://ferroterm.eu"));
        assert!(banner.lines().all(|line| line.chars().count() <= 100));
        assert_eq!(banner.lines().count(), 8);
    }
}
