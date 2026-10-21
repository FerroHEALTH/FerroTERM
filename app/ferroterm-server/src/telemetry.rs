// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Console logging: the format choice and the subscriber.
//!
//! Two formats over `tracing`: `pretty` for a person (colour when the
//! console is a terminal, aligned fields) and `json` for a log pipeline (one
//! object per line). `auto`, the default, picks `pretty` on a terminal and
//! `json` otherwise, so a container gets machine lines without configuration.
//! `RUST_LOG` is the filter; the HTTP stack's own crates default to `warn`.
//! No spec governs the console: our own design.

use std::io::{self, Write};
use std::str::FromStr;

use tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// The environment variable choosing the log format.
pub const FORMAT_ENV: &str = "FERROTERM_LOG_FORMAT";
/// The environment variable carrying the `tracing` filter.
pub const FILTER_ENV: &str = "RUST_LOG";
/// The filter used when `RUST_LOG` is unset.
pub const DEFAULT_FILTER: &str = "info,hyper=warn,tower=warn,h2=warn";

/// The requested format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogFormat {
    /// `pretty` on a terminal, `json` otherwise.
    #[default]
    Auto,
    /// One JSON object per line.
    Json,
    /// Human-readable lines.
    Pretty,
}

/// The format after `auto` is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedFormat {
    /// One JSON object per line.
    Json,
    /// Human-readable lines.
    Pretty,
}

/// A format name that is not `auto`, `json`, or `pretty`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a log format; use auto, json, or pretty")]
pub struct FormatError(pub String);

impl FromStr for LogFormat {
    type Err = FormatError;

    fn from_str(text: &str) -> Result<Self, FormatError> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" | "" => Ok(Self::Auto),
            "json" => Ok(Self::Json),
            "pretty" => Ok(Self::Pretty),
            other => Err(FormatError(other.to_owned())),
        }
    }
}

impl LogFormat {
    /// Decides `auto` from whether stdout is a terminal; the one place the
    /// rule lives, so the banner and the log layer agree.
    #[must_use]
    pub const fn resolve(self, stdout_is_terminal: bool) -> ResolvedFormat {
        match self {
            Self::Pretty => ResolvedFormat::Pretty,
            Self::Auto if stdout_is_terminal => ResolvedFormat::Pretty,
            Self::Json | Self::Auto => ResolvedFormat::Json,
        }
    }
}

/// A failure to install the subscriber.
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    /// A subscriber is already installed in this process.
    #[error("a log subscriber is already installed")]
    AlreadyInstalled(#[from] tracing_subscriber::util::TryInitError),
}

/// Builds the subscriber for `format` with `filter` writing through `writer`.
///
/// `ansi` colours the pretty output; JSON never carries colour. A filter that
/// does not parse falls back to [`DEFAULT_FILTER`] so a typo in `RUST_LOG`
/// does not stop the process (the fallback is logged once the subscriber runs).
pub fn subscriber<W>(
    format: ResolvedFormat,
    filter: &str,
    ansi: bool,
    writer: W,
) -> impl Subscriber + Send + Sync
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = match format {
        ResolvedFormat::Json => Box::new(
            tracing_subscriber::fmt::layer()
                .json()
                .flatten_event(true)
                .with_current_span(false)
                .with_span_list(false)
                .with_writer(writer),
        ),
        ResolvedFormat::Pretty => Box::new(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_ansi(ansi)
                .with_writer(LineSafe(writer)),
        ),
    };
    tracing_subscriber::registry().with(filter).with(layer)
}

/// Installs the process-wide subscriber on stdout.
///
/// Returns the resolved format so the caller knows whether a person is reading.
///
/// # Errors
///
/// Returns [`TelemetryError::AlreadyInstalled`] when a subscriber exists.
pub fn init(
    format: LogFormat,
    filter: &str,
    stdout_is_terminal: bool,
) -> Result<ResolvedFormat, TelemetryError> {
    let resolved = format.resolve(stdout_is_terminal);
    // An explicit `pretty` keeps its colour even into a pipe (a person asked
    // for it, for example under `docker compose logs`); `auto` follows the terminal.
    let ansi = matches!(format, LogFormat::Pretty) || stdout_is_terminal;
    subscriber(resolved, filter, ansi, io::stdout).try_init()?;
    if EnvFilter::try_new(filter).is_err() {
        tracing::warn!(
            filter,
            fallback = DEFAULT_FILTER,
            "the log filter does not parse; using the default"
        );
    }
    Ok(resolved)
}

/// A writer that keeps every log line on one line.
///
/// A carriage return or line feed inside a field value becomes `\r` or `\n`,
/// so a value cannot forge a log line (OWASP Logging Cheat Sheet, log
/// injection). The JSON format escapes these itself; only the text format
/// needs it.
#[derive(Debug, Clone)]
pub struct LineSafe<M>(pub M);

impl<'a, M> MakeWriter<'a> for LineSafe<M>
where
    M: MakeWriter<'a>,
{
    type Writer = LineSafeWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        LineSafeWriter(self.0.make_writer())
    }
}

/// The writer [`LineSafe`] hands out.
#[derive(Debug)]
pub struct LineSafeWriter<W>(W);

impl<W: Write> Write for LineSafeWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let body = buf.strip_suffix(b"\n").unwrap_or(buf);
        let mut escaped = Vec::with_capacity(buf.len() + 8);
        for byte in body {
            match byte {
                b'\n' => escaped.extend_from_slice(b"\\n"),
                b'\r' => escaped.extend_from_slice(b"\\r"),
                other => escaped.push(*other),
            }
        }
        if body.len() < buf.len() {
            escaped.push(b'\n');
        }
        self.0.write_all(&escaped)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::{LineSafeWriter, LogFormat, ResolvedFormat};
    use std::io::Write;

    #[test]
    fn auto_follows_the_terminal_and_explicit_formats_do_not() {
        assert_eq!(LogFormat::Auto.resolve(true), ResolvedFormat::Pretty);
        assert_eq!(LogFormat::Auto.resolve(false), ResolvedFormat::Json);
        assert_eq!(LogFormat::Json.resolve(true), ResolvedFormat::Json);
        assert_eq!(LogFormat::Pretty.resolve(false), ResolvedFormat::Pretty);
        assert_eq!("JSON".parse::<LogFormat>(), Ok(LogFormat::Json));
        assert_eq!("".parse::<LogFormat>(), Ok(LogFormat::Auto));
        assert!("xml".parse::<LogFormat>().is_err());
    }

    #[test]
    fn interior_line_breaks_are_escaped_and_the_terminator_kept() {
        let mut out = Vec::new();
        LineSafeWriter(&mut out)
            .write_all(b"a\r\nb\n")
            .expect("writes");
        assert_eq!(out, b"a\\r\\nb\n");
    }
}
