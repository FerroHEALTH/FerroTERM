// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The machine a record was taken on: its platform, its memory, and what it
//! was doing when the run started.

use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// The machine a record was taken on.
#[derive(Debug, Serialize)]
pub(super) struct Machine {
    pub(super) os: String,
    pub(super) arch: String,
    pub(super) cpu: String,
    pub(super) memory_bytes: u64,
    pub(super) container: bool,
    /// The one-minute load average when the run started.
    ///
    /// A busy machine gives the operating system's readings rather than the
    /// server's: with an IDE indexing and a container runtime resident, a
    /// served edition read 155 MB where a settled machine read 830 MB, and an
    /// ingest took 69 s where a settled machine took 25 s. A record states
    /// what the machine was doing so a reader can tell one from the other
    /// (#512).
    pub(super) load_average_1m: Option<f64>,
}

/// The machine, from `sysctl` on macOS and `/proc` on Linux.
pub(super) fn machine() -> Machine {
    let sysctl = |name: &str| {
        Command::new("sysctl")
            .args(["-n", name])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let proc_field = |path: &str, key: &str| {
        std::fs::read_to_string(path).ok().and_then(|text| {
            text.lines()
                .find(|l| l.starts_with(key))
                .and_then(|l| l.split(':').nth(1))
                .map(|v| v.trim().to_owned())
        })
    };
    let (cpu, memory_bytes) = if cfg!(target_os = "macos") {
        (
            sysctl("machdep.cpu.brand_string").unwrap_or_default(),
            sysctl("hw.memsize")
                .and_then(|m| m.parse().ok())
                .unwrap_or(0),
        )
    } else {
        (
            // An aarch64 /proc/cpuinfo names no model; the implementer and part
            // fields are what it has.
            proc_field("/proc/cpuinfo", "model name")
                .or_else(|| proc_field("/proc/cpuinfo", "Hardware"))
                .or_else(|| {
                    let implementer = proc_field("/proc/cpuinfo", "CPU implementer")?;
                    let vendor = match implementer.as_str() {
                        "0x41" => "Arm",
                        "0x51" => "Qualcomm",
                        "0x61" => "Apple",
                        "0xc0" => "Ampere",
                        _ => "unknown vendor",
                    };
                    Some(format!(
                        "{vendor} {} (implementer {implementer}, part {})",
                        std::env::consts::ARCH,
                        proc_field("/proc/cpuinfo", "CPU part")?
                    ))
                })
                .unwrap_or_else(|| "unknown".to_owned()),
            proc_field("/proc/meminfo", "MemTotal")
                .and_then(|m| {
                    m.split_whitespace()
                        .next()
                        .and_then(|kb| kb.parse::<u64>().ok())
                })
                .map_or(0, |kb| kb * 1024),
        )
    };
    Machine {
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        cpu,
        memory_bytes,
        container: Path::new("/.dockerenv").exists() || std::env::var_os("container").is_some(),
        load_average_1m: load_average(),
    }
}

/// The one-minute load average, from `uptime`.
///
/// `getloadavg` needs `unsafe` or a platform crate, and the line `uptime`
/// prints carries the same three numbers on both platforms this runs on.
fn load_average() -> Option<f64> {
    let output = Command::new("uptime").output().ok()?;
    first_load(&String::from_utf8_lossy(&output.stdout))
}

/// The first of the three load averages in a line `uptime` printed.
///
/// macOS writes `load averages: 4.63 7.33 8.36` and Linux writes
/// `load average: 0.00, 0.01, 0.05`, so the separator is a comma on one and a
/// space on the other.
fn first_load(text: &str) -> Option<f64> {
    let (_, averages) = text.split_once("load average")?;
    averages
        .trim_start_matches(['s', ':', ' '])
        .split([',', ' '])
        .find(|field| !field.is_empty())?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::first_load;

    #[test]
    fn the_load_average_is_read_on_both_platforms() {
        assert_eq!(
            first_load("7:08  up 10 days, 1 user, load averages: 4.63 7.33 8.36"),
            Some(4.63),
            "macOS separates the three with spaces"
        );
        assert_eq!(
            first_load(" 07:08:11 up 3 days,  2 users,  load average: 0.00, 0.01, 0.05"),
            Some(0.00),
            "Linux separates them with commas"
        );
        assert_eq!(
            first_load("a line that says nothing about the load"),
            None,
            "a record states no load rather than a wrong one"
        );
    }
}
