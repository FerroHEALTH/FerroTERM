// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Putting what a run staged in front of the server, and taking it back.
//!
//! A staged item reaches the server with one rename, which the filesystem
//! performs whole: a release directory appears under the index root with its
//! manifest already in it, so a reload that runs at that moment sees either
//! the old set or the new one. A resource file that replaces one already
//! served is moved aside first, so a refused reload can put back exactly what
//! was there.
//!
//! Nothing here deletes anything. A rollback moves files back where they came
//! from, and only [`crate::retention`] removes a release.
//!
//! No FHIR specification governs this: our own design.

use std::path::{Path, PathBuf};

use crate::state::{Lane, Staged};

/// A staged item that could not be put in front of the server.
#[derive(Debug, thiserror::Error)]
pub enum ActivateError {
    /// The staged item is not where the run left it.
    #[error("the staged item {path} is gone")]
    Missing {
        /// Where the item was expected.
        path: PathBuf,
    },
    /// A release directory of that name is already served.
    #[error("{path} is already served; nothing is overwritten")]
    Occupied {
        /// The directory that is already there.
        path: PathBuf,
    },
    /// The move did not work.
    #[error("cannot move {from} to {to}")]
    Move {
        /// Where the item was.
        from: PathBuf,
        /// Where it was going.
        to: PathBuf,
        /// Why the move failed.
        #[source]
        source: std::io::Error,
    },
}

/// One staged item that reached the server, and what it displaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The item that was moved.
    pub item: Staged,
    /// The file it replaced, kept until the reload succeeds.
    pub displaced: Option<PathBuf>,
}

/// Moves every staged item to where the server reads it.
///
/// On the first failure everything already moved is put back, so the served
/// set is either wholly the old one or wholly the new one.
///
/// # Errors
///
/// Returns [`ActivateError::Missing`] when a staged item is gone,
/// [`ActivateError::Occupied`] when a release directory of that name is
/// already served, and [`ActivateError::Move`] when a move does not work.
pub async fn place(items: &[Staged]) -> Result<Vec<Placed>, ActivateError> {
    let mut placed = Vec::with_capacity(items.len());
    for item in items {
        match place_one(item).await {
            Ok(one) => placed.push(one),
            Err(error) => {
                undo(&placed).await;
                return Err(error);
            }
        }
    }
    Ok(placed)
}

/// Moves one staged item to where the server reads it.
async fn place_one(item: &Staged) -> Result<Placed, ActivateError> {
    if !exists(&item.path).await {
        return Err(ActivateError::Missing {
            path: item.path.clone(),
        });
    }
    if item.lane == Lane::Index && exists(&item.target).await {
        return Err(ActivateError::Occupied {
            path: item.target.clone(),
        });
    }
    let mut displaced = None;
    if item.lane == Lane::Resource && exists(&item.target).await {
        let Some(keep) = item.replaced.clone() else {
            return Err(ActivateError::Occupied {
                path: item.target.clone(),
            });
        };
        create_parent(&keep).await?;
        rename(&item.target, &keep).await?;
        displaced = Some(keep);
    }
    create_parent(&item.target).await?;
    match rename(&item.path, &item.target).await {
        Ok(()) => Ok(Placed {
            item: item.clone(),
            displaced,
        }),
        Err(error) => {
            if let Some(keep) = displaced {
                restore(&keep, &item.target).await;
            }
            Err(error)
        }
    }
}

/// Puts everything `placed` moved back where it came from.
///
/// A move that fails here is logged and the rest is still attempted: leaving
/// the remaining items in place would be a half-rolled-back served set.
pub async fn undo(placed: &[Placed]) {
    for one in placed.iter().rev() {
        restore(&one.item.target, &one.item.path).await;
        if let Some(keep) = one.displaced.as_ref() {
            restore(keep, &one.item.target).await;
        }
    }
}

/// Moves `from` back to `to`, reporting a move that failed through `tracing`.
async fn restore(from: &Path, to: &Path) {
    if let Err(error) = tokio::fs::rename(from, to).await {
        tracing::error!(
            from = %from.display(),
            to = %to.display(),
            %error,
            "the activation could not be rolled back"
        );
    }
}

/// Whether the path is there at all, a file or a directory.
async fn exists(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path).await.is_ok()
}

/// Creates the directory `path` will sit in.
async fn create_parent(path: &Path) -> Result<(), ActivateError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|source| ActivateError::Move {
            from: path.to_path_buf(),
            to: parent.to_path_buf(),
            source,
        })
}

/// Moves `from` onto `to`.
async fn rename(from: &Path, to: &Path) -> Result<(), ActivateError> {
    tokio::fs::rename(from, to)
        .await
        .map_err(|source| ActivateError::Move {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            source,
        })
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn, reason = "test assertions")]
mod tests {
    use super::{place, undo};
    use crate::state::{Lane, Staged};

    fn staged(lane: Lane, from: &std::path::Path, to: &std::path::Path) -> Staged {
        Staged {
            source: String::from("reference"),
            lane,
            path: from.to_path_buf(),
            target: to.to_path_buf(),
            replaced: None,
            canonical: String::from("https://example.invalid/vs"),
            version: String::from("1"),
            date: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    #[tokio::test]
    async fn a_release_directory_is_renamed_whole() -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let from = dir.path().join("staging").join("release");
        let to = dir.path().join("index").join("release");
        std::fs::create_dir_all(&from)?;
        std::fs::write(from.join("manifest.json"), "{}")?;
        let placed = place(&[staged(Lane::Index, &from, &to)]).await?;
        assert!(
            to.join("manifest.json").is_file(),
            "the directory arrives with its manifest already in it"
        );
        undo(&placed).await;
        assert!(
            from.join("manifest.json").is_file(),
            "a rollback puts the directory back in staging"
        );
        assert!(!to.exists(), "and leaves the index root as it was");
        Ok(())
    }

    #[tokio::test]
    async fn a_resource_that_replaces_one_keeps_the_old_bytes()
    -> Result<(), Box<dyn core::error::Error>> {
        let dir = tempfile::tempdir()?;
        let from = dir.path().join("staging").join("a.json");
        let to = dir.path().join("codesystems").join("a.json");
        let keep = dir.path().join("replaced").join("a.json");
        std::fs::create_dir_all(dir.path().join("staging"))?;
        std::fs::create_dir_all(dir.path().join("codesystems"))?;
        std::fs::write(&from, "new")?;
        std::fs::write(&to, "old")?;
        let mut item = staged(Lane::Resource, &from, &to);
        item.replaced = Some(keep.clone());
        let placed = place(&[item]).await?;
        assert_eq!(
            std::fs::read_to_string(&to)?,
            "new",
            "the new file is served"
        );
        assert_eq!(
            std::fs::read_to_string(&keep)?,
            "old",
            "the file it replaced is kept until the reload succeeds"
        );
        undo(&placed).await;
        assert_eq!(
            std::fs::read_to_string(&to)?,
            "old",
            "a rollback puts back exactly what was served"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_release_of_that_name_is_never_overwritten() -> Result<(), Box<dyn core::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let from = dir.path().join("staging").join("release");
        let to = dir.path().join("index").join("release");
        std::fs::create_dir_all(&from)?;
        std::fs::create_dir_all(&to)?;
        std::fs::write(to.join("manifest.json"), "served")?;
        let placed = place(&[staged(Lane::Index, &from, &to)]).await;
        assert!(
            placed.is_err(),
            "a release already served is left exactly where it is"
        );
        assert_eq!(
            std::fs::read_to_string(to.join("manifest.json"))?,
            "served",
            "and its bytes are untouched"
        );
        Ok(())
    }

    #[tokio::test]
    async fn one_failure_rolls_the_whole_activation_back() -> Result<(), Box<dyn core::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let first_from = dir.path().join("staging").join("first");
        let first_to = dir.path().join("index").join("first");
        std::fs::create_dir_all(&first_from)?;
        let second = staged(
            Lane::Index,
            &dir.path().join("staging").join("gone"),
            &dir.path().join("index").join("second"),
        );
        let placed = place(&[staged(Lane::Index, &first_from, &first_to), second]).await;
        assert!(placed.is_err(), "the second item is not there");
        assert!(
            first_from.is_dir() && !first_to.exists(),
            "the first item went back to staging"
        );
        Ok(())
    }
}
