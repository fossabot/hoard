//! Utilities for checking whether files differ.
//!
//! Unified diffs are optionally available for text files. Following Git's example,
//! non-text binary files can only be detected as differing or the same.
use std::io::Read;
use std::path::Path;
use std::{fs, io};

use similar::{ChangeTag, TextDiff};

const CONTEXT_RADIUS: usize = 5;

#[derive(Debug, Clone, PartialEq)]
enum FileContent {
    Text(String),
    Binary(Vec<u8>),
    Missing,
}

impl FileContent {
    fn read(file: fs::File) -> io::Result<Self> {
        let bytes: Vec<u8> = file.bytes().collect::<io::Result<_>>()?;
        match String::from_utf8(bytes) {
            Ok(s) => Ok(Self::Text(s)),
            Err(err) => Ok(Self::Binary(err.into_bytes())),
        }
    }

    fn into_bytes(self) -> Option<Vec<u8>> {
        match self {
            Self::Text(s) => Some(s.into_bytes()),
            Self::Binary(v) => Some(v),
            Self::Missing => None,
        }
    }
}

#[allow(variant_size_differences)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Diff {
    /// Text content differs. Contains the generated unified diff.
    Text(String),
    /// Binary content differs. Also occurs if a file changes between text and binary formats.
    Binary,
    /// Content is the same, but permissions differ.
    Permissions(fs::Permissions, fs::Permissions),
    /// The left path to diff_files did not exist, but the right path did.
    LeftNotExists,
    /// The left path to diff_paths existed, but the right path did not.
    RightNotExists,
}

fn content_and_meta_for(path: &Path) -> io::Result<(FileContent, Option<fs::Metadata>)> {
    match fs::File::open(path) {
        Ok(file) => {
            let meta = file.metadata()?;
            let content = FileContent::read(file)?;
            Ok((content, Some(meta)))
        }
        Err(err) => match err.kind() {
            io::ErrorKind::NotFound => Ok((FileContent::Missing, None)),
            _ => Err(err),
        },
    }
}

pub(crate) fn diff_files(left_path: &Path, right_path: &Path) -> io::Result<Option<Diff>> {
    let (left, left_meta) = content_and_meta_for(left_path)?;
    let (right, right_meta) = content_and_meta_for(right_path)?;

    let permissions_diff = if let (Some(left_meta), Some(right_meta)) = (left_meta, right_meta) {
        let left_perms = left_meta.permissions();
        let right_perms = right_meta.permissions();
        (left_perms != right_perms).then(|| Diff::Permissions(left_perms, right_perms))
    } else {
        None
    };

    let diff = match (left, right) {
        (FileContent::Missing, FileContent::Missing) => None,
        (FileContent::Missing, _) => Some(Diff::LeftNotExists),
        (_, FileContent::Missing) => Some(Diff::RightNotExists),
        (FileContent::Text(left_text), FileContent::Text(right_text)) => {
            let text_diff = TextDiff::from_lines(&left_text, &right_text);

            let has_diff = !text_diff
                .iter_all_changes()
                .all(|op| op.tag() == ChangeTag::Equal);

            if has_diff {
                let udiff = text_diff
                    .unified_diff()
                    .context_radius(CONTEXT_RADIUS)
                    .header(&left_path.to_string_lossy(), &right_path.to_string_lossy())
                    .to_string();
                Some(Diff::Text(udiff))
            } else {
                permissions_diff
            }
        }
        (left, right) => {
            let left = left.into_bytes();
            let right = right.into_bytes();

            left.into_iter()
                .zip(right)
                .any(|(l, r)| l != r)
                .then(|| Diff::Binary)
                .or(permissions_diff)
        }
    };

    Ok(diff)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_diff_non_existent_files() {
        let left_path = PathBuf::from("/does/not/exist");
        let right_path = PathBuf::from("/also/does/not/exist");
        let diff = diff_files(&left_path, &right_path).expect("diff should not fail");

        assert!(diff.is_none());
    }

    mod file_content {
        use super::*;

        #[test]
        fn test_text_into_bytes() {
            let string_content = String::from("text content");
            let s = FileContent::Text(string_content.clone());
            assert_eq!(s.into_bytes(), Some(string_content.into_bytes()));
        }

        #[test]
        fn test_binary_into_bytes() {
            let bytes = vec![23u8, 244u8, 0u8, 12u8, 17u8];
            let b = FileContent::Binary(bytes.clone());
            assert_eq!(b.into_bytes(), Some(bytes));
        }

        #[test]
        fn test_missing_into_bytes() {
            assert_eq!(FileContent::Missing.into_bytes(), None);
        }
    }
}
