// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How much memory a process holds, read the same way by every measurement
//! here.
//!
//! macOS is asked for the physical footprint, which counts a page the kernel
//! compressed away; Linux is asked for the resident set, which is the same
//! thing there. `ps -o rss=` on macOS counts only what is resident and
//! uncompressed: one served edition read 379 MB and then 15 MB two seconds
//! later while holding the same structures, against a physical footprint of
//! 896 MB throughout (#512).

use std::process::Command;

/// How many readings a median takes.
///
/// An odd count, so the median is a reading rather than an average of two.
const SAMPLES: usize = 5;

/// How long between two readings.
const INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// The memory process `pid` holds, in bytes.
#[must_use]
pub fn of_process(pid: u32) -> Option<u64> {
    if cfg!(target_os = "macos") {
        let output = Command::new("footprint")
            .args(["-p", &pid.to_string()])
            .output()
            .ok()?;
        return physical_footprint(&String::from_utf8_lossy(&output.stdout));
    }
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kilobytes = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(kilobytes * 1024)
}

/// The median of five readings of process `pid`, a fifth of a second
/// apart.
///
/// A single reading taken the moment a process finished other work is not the
/// figure it settles at (#304).
#[must_use]
pub fn median_of_process(pid: u32) -> Option<u64> {
    let mut samples: Vec<u64> = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        if sample > 0 {
            std::thread::sleep(INTERVAL);
        }
        samples.push(of_process(pid)?);
    }
    samples.sort_unstable();
    // The middle of an odd-length list, which SAMPLES fixes.
    samples.get(SAMPLES.div_euclid(2)).copied()
}

/// The `phys_footprint` line of a `footprint` report, in bytes.
///
/// The tool writes it as a rounded quantity with its unit (`896 MB`), so the
/// unit is read rather than assumed.
#[must_use]
pub fn physical_footprint(report: &str) -> Option<u64> {
    let line = report
        .lines()
        .find(|line| line.trim_start().starts_with("phys_footprint:"))?;
    let (_, value) = line.split_once(':')?;
    let mut fields = value.split_whitespace();
    let amount: f64 = fields.next()?.replace(',', "").parse().ok()?;
    let scale = match fields.next()?.to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "K" | "KB" => 1024.0,
        "M" | "MB" => 1024.0 * 1024.0,
        "G" | "GB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    // Rendered and re-read rather than cast, because `as` on a float is a
    // silent truncation this workspace denies, and a footprint is a whole
    // number of bytes either way.
    format!("{:.0}", amount * scale).parse().ok()
}

#[cfg(test)]
mod tests {
    use super::physical_footprint;

    #[test]
    fn the_physical_footprint_is_read_with_its_unit() {
        let report =
            "Auxiliary data:\n    phys_footprint: 896 MB\n    phys_footprint_peak: 896 MB\n";
        assert_eq!(
            physical_footprint(report),
            Some(896 * 1024 * 1024),
            "the unit is read rather than assumed"
        );
        assert_eq!(
            physical_footprint("    phys_footprint: 1.5 GB\n"),
            Some(1_610_612_736),
            "a footprint is reported with a fraction once it passes a gigabyte"
        );
        assert_eq!(
            physical_footprint("    phys_footprint: 512 KB\n"),
            Some(512 * 1024)
        );
        assert_eq!(
            physical_footprint("nothing about a footprint here"),
            None,
            "a record states no memory rather than a wrong figure"
        );
    }
}
