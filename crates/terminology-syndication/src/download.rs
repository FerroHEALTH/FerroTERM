// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Streaming one content item to a file and verifying its digest.
//!
//! The bytes never sit in memory as a whole: a release package runs to
//! hundreds of megabytes. They go to a partial file beside the destination,
//! and the destination appears only once the digest the feed advertised
//! matches what was written.

use std::path::{Path, PathBuf};

use sha2::Digest as _;
use tokio::io::AsyncWriteExt as _;

use crate::model::Checksum;

/// A content item that could not be fetched.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// The request itself failed.
    #[error("the download request to {url} failed")]
    Request {
        /// The address the request was sent to.
        url: String,
        /// Why the request failed.
        #[source]
        source: reqwest::Error,
    },
    /// The server answered with a status that is not a success.
    #[error("{url} answered {status}")]
    Status {
        /// The address the request was sent to.
        url: String,
        /// The status the server answered with.
        status: reqwest::StatusCode,
    },
    /// The bytes could not be written, or the partial file could not be moved.
    #[error("{path} could not be written")]
    Write {
        /// The file being written.
        path: PathBuf,
        /// Why the write failed.
        #[source]
        source: std::io::Error,
    },
    /// The bytes that arrived do not carry the digest the feed advertised.
    #[error("{url} is {computed}, and the feed advertises {expected}")]
    ChecksumMismatch {
        /// The address the bytes came from.
        url: String,
        /// The digest the feed advertised.
        expected: Checksum,
        /// The digest of the bytes that arrived, in the same algorithm.
        computed: String,
    },
}

/// A content item that arrived, with its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// Where the bytes were written.
    pub path: PathBuf,
    /// How many bytes arrived.
    pub bytes: u64,
    /// The digest that was verified, or computed when none was advertised.
    pub checksum: Checksum,
}

/// Streams `request` to `destination` and verifies `expected` before returning.
///
/// The bytes go to `<destination>.part` while they arrive. On a digest match
/// that file is renamed onto `destination`; on a mismatch it is removed, so a
/// refused download leaves nothing behind for a later run to mistake for
/// content.
///
/// # Errors
///
/// Returns [`DownloadError::Request`] when the request fails,
/// [`DownloadError::Status`] when the server answers with a status that is not
/// a success, [`DownloadError::Write`] when the file cannot be written or
/// moved, and [`DownloadError::ChecksumMismatch`] when the bytes that arrived
/// do not carry the advertised digest.
pub async fn to_file(
    request: reqwest::RequestBuilder,
    destination: &Path,
    expected: &Checksum,
) -> Result<Fetched, DownloadError> {
    write(request, destination, Some(expected)).await
}

/// Streams `request` to `destination` and answers the SHA-256 of what arrived.
///
/// This is for a source that advertises no digest, such as a FHIR API that
/// serves a resource by its identifier. The digest is computed rather than
/// verified, so the caller can still record what it took.
///
/// # Errors
///
/// Returns [`DownloadError::Request`] when the request fails,
/// [`DownloadError::Status`] when the server answers with a status that is not
/// a success, and [`DownloadError::Write`] when the file cannot be written or
/// moved.
pub async fn to_file_unverified(
    request: reqwest::RequestBuilder,
    destination: &Path,
) -> Result<Fetched, DownloadError> {
    write(request, destination, None).await
}

async fn write(
    request: reqwest::RequestBuilder,
    destination: &Path,
    expected: Option<&Checksum>,
) -> Result<Fetched, DownloadError> {
    let url = request_url(&request);
    let mut response = request
        .send()
        .await
        .map_err(|source| DownloadError::Request {
            url: url.clone(),
            source,
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::Status { url, status });
    }

    let partial = partial_path(destination);
    let mut digest =
        expected.map_or_else(|| Digest::Sha256(sha2::Sha256::new()), Digest::for_checksum);
    let mut file =
        tokio::fs::File::create(&partial)
            .await
            .map_err(|source| DownloadError::Write {
                path: partial.clone(),
                source,
            })?;
    let written = stream(&mut response, &mut file, &mut digest, &url, &partial).await;
    let flushed = file.flush().await.map_err(|source| DownloadError::Write {
        path: partial.clone(),
        source,
    });
    drop(file);
    let bytes = match written.and_then(|bytes| flushed.map(|()| bytes)) {
        Ok(bytes) => bytes,
        Err(error) => {
            discard(&partial).await;
            return Err(error);
        }
    };

    let computed = digest.finish();
    let checksum = match expected {
        Some(expected) if !expected.matches(&computed) => {
            discard(&partial).await;
            return Err(DownloadError::ChecksumMismatch {
                url,
                expected: expected.clone(),
                computed,
            });
        }
        Some(expected) => expected.clone(),
        None => Checksum::Sha256(computed),
    };
    tokio::fs::rename(&partial, destination)
        .await
        .map_err(|source| DownloadError::Write {
            path: destination.to_path_buf(),
            source,
        })?;
    Ok(Fetched {
        path: destination.to_path_buf(),
        bytes,
        checksum,
    })
}

async fn stream(
    response: &mut reqwest::Response,
    file: &mut tokio::fs::File,
    digest: &mut Digest,
    url: &str,
    partial: &Path,
) -> Result<u64, DownloadError> {
    let mut bytes: u64 = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|source| DownloadError::Request {
            url: url.to_owned(),
            source,
        })?
    {
        digest.update(&chunk);
        bytes = bytes.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
        file.write_all(&chunk)
            .await
            .map_err(|source| DownloadError::Write {
                path: partial.to_path_buf(),
                source,
            })?;
    }
    Ok(bytes)
}

/// Removes a partial file, reporting a removal that failed through `tracing`.
async fn discard(partial: &Path) {
    if let Err(error) = tokio::fs::remove_file(partial).await {
        tracing::warn!(
            path = %partial.display(),
            %error,
            "the partial download could not be removed"
        );
    }
}

fn partial_path(destination: &Path) -> PathBuf {
    let mut name = destination.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

fn request_url(request: &reqwest::RequestBuilder) -> String {
    request
        .try_clone()
        .and_then(|clone| clone.build().ok())
        .map_or_else(
            || String::from("the content item"),
            |built| built.url().to_string(),
        )
}

/// The running digest of the bytes as they arrive.
#[derive(Debug)]
enum Digest {
    Sha256(sha2::Sha256),
    Md5(md5::Md5),
}

impl Digest {
    fn for_checksum(checksum: &Checksum) -> Self {
        match checksum {
            Checksum::Sha256(_) => Self::Sha256(sha2::Sha256::new()),
            Checksum::Md5(_) => Self::Md5(md5::Md5::new()),
        }
    }

    fn update(&mut self, chunk: &[u8]) {
        match self {
            Self::Sha256(hasher) => hasher.update(chunk),
            Self::Md5(hasher) => hasher.update(chunk),
        }
    }

    fn finish(self) -> String {
        match self {
            Self::Sha256(hasher) => hex(&hasher.finalize()),
            Self::Md5(hasher) => hex(&hasher.finalize()),
        }
    }
}

/// The lowercase hexadecimal form of a digest.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        out.push(nibble(byte >> 4));
        out.push(nibble(byte & 0x0f));
    }
    out
}

/// The hexadecimal digit of one nibble, which is always below 16.
fn nibble(value: u8) -> char {
    if value < 10 {
        char::from(b'0' + value)
    } else {
        char::from(b'a' + value - 10)
    }
}
