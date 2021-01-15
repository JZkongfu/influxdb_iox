//! This module contains code for abstracting object locations that work
//! across different backing implementations and platforms.
use itertools::Itertools;

use std::{mem, path::PathBuf};

/// Paths that came from or are to be used in cloud-based object storage
pub mod cloud;

/// Paths that come from or are to be used in file-based object storage
pub mod file;

/// Maximally processed storage-independent paths.
pub mod parsed;
use parsed::DirsAndFileName;

mod parts;
use parts::PathPart;

/// Universal interface for handling paths and locations for objects and
/// directories in the object store.
///
/// It allows IOx to be completely decoupled from the underlying object store
/// implementations.
///
/// Deliberately does not implement `Display` or `ToString`! Use one of the
/// converters.
pub trait Osp: Default + PartialEq + Eq + Send + Sync + 'static {
    /// Set the file name of this path
    fn set_file_name(&mut self, part: impl Into<String>);

    /// Add a part to the end of the path's directories, encoding any restricted
    /// characters.
    fn push_dir(&mut self, part: impl Into<String>);

    /// Push a bunch of parts as directories in one go.
    fn push_all_dirs<'a>(&mut self, parts: impl AsRef<[&'a str]>);

    /// Convert an `ObjectStorePath` to a `String` according to the appropriate
    /// implementation. Suitable for printing; not suitable for sending to
    /// APIs
    fn display(&self) -> String;
}

/// Slated for removal
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct ObjectStorePath {
    inner: PathRepresentation,
}

impl From<ObjectStorePath> for cloud::CloudPath {
    fn from(object_store_path: ObjectStorePath) -> Self {
        use PathRepresentation::*;
        match object_store_path.inner {
            RawCloud(path) => cloud::CloudPath::raw(path),
            RawPathBuf(_) => unreachable!(),
            Parts(parts) => parts.into(),
        }
    }
}

impl ObjectStorePath {
    /// For use when receiving a path from an object store API directly, not
    /// when building a path. Assumes DELIMITER is the separator.
    ///
    /// TODO: This should only be available to cloud storage
    pub fn from_cloud_unchecked(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            inner: PathRepresentation::RawCloud(path),
        }
    }

    /// For use when receiving a path from a filesystem directly, not
    /// when building a path. Uses the standard library's path splitting
    /// implementation to separate into parts.
    ///
    /// TODO: This should only be available to file storage
    pub fn from_path_buf_unchecked(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            inner: PathRepresentation::RawPathBuf(path),
        }
    }

    /// Add a part to the end of the path, encoding any restricted characters.
    pub fn push_dir(&mut self, part: impl Into<String>) {
        self.inner = mem::take(&mut self.inner).push_dir(part);
    }

    /// Add a `PathPart` to the end of the path.
    pub fn push_part_as_dir(&mut self, part: &PathPart) {
        self.inner = mem::take(&mut self.inner).push_part_as_dir(part);
    }

    /// Set the file name of this path
    pub fn set_file_name(&mut self, part: impl Into<String>) {
        self.inner = mem::take(&mut self.inner).set_file_name(part);
    }

    /// Push a bunch of parts as directories in one go.
    pub fn push_all_dirs<'a>(&mut self, parts: impl AsRef<[&'a str]>) {
        self.inner = mem::take(&mut self.inner).push_all_dirs(parts);
    }

    /// Pops a part from the path and returns it, or `None` if it's empty.
    pub fn pop(&mut self) -> Option<&PathPart> {
        unimplemented!()
    }

    /// Returns true if the directories in `prefix` are the same as the starting
    /// directories of `self`.
    pub fn prefix_matches(&self, prefix: &Self) -> bool {
        use PathRepresentation::*;
        match (&self.inner, &prefix.inner) {
            (Parts(self_parts), Parts(other_parts)) => self_parts.prefix_matches(&other_parts),
            (Parts(self_parts), _) => {
                let prefix_parts: DirsAndFileName = prefix.into();
                self_parts.prefix_matches(&prefix_parts)
            }
            (_, Parts(prefix_parts)) => {
                let self_parts: DirsAndFileName = self.into();
                self_parts.prefix_matches(&prefix_parts)
            }
            _ => {
                let self_parts: DirsAndFileName = self.into();
                let prefix_parts: DirsAndFileName = prefix.into();
                self_parts.prefix_matches(&prefix_parts)
            }
        }
    }
}

impl From<&'_ DirsAndFileName> for ObjectStorePath {
    fn from(other: &'_ DirsAndFileName) -> Self {
        other.clone().into()
    }
}

impl From<DirsAndFileName> for ObjectStorePath {
    fn from(other: DirsAndFileName) -> Self {
        Self {
            inner: PathRepresentation::Parts(other),
        }
    }
}

impl Osp for ObjectStorePath {
    fn push_dir(&mut self, _dir: impl Into<String>) {
        todo!()
    }

    fn push_all_dirs<'a>(&mut self, parts: impl AsRef<[&'a str]>) {
        todo!()
    }

    fn set_file_name(&mut self, _file: impl Into<String>) {
        todo!()
    }

    fn display(&self) -> String {
        todo!()
        // match &self.0 {
        //     AmazonS3(_) | GoogleCloudStorage(_) | InMemory(_) |
        // MicrosoftAzure(_) => {         path::cloud::CloudConverter::
        // convert(path)     }
        //     File(_) => path::file::FileConverter::convert(path)
        //         .display()
        //         .to_string(),
        // }
    }
}

#[derive(Clone, Eq, Debug)]
enum PathRepresentation {
    RawCloud(String),
    RawPathBuf(PathBuf),
    Parts(DirsAndFileName),
}

impl Default for PathRepresentation {
    fn default() -> Self {
        Self::Parts(DirsAndFileName::default())
    }
}

impl PathRepresentation {
    /// Add a part to the end of the path's directories, encoding any restricted
    /// characters.
    fn push_dir(self, part: impl Into<String>) -> Self {
        let mut dirs_and_file_name: DirsAndFileName = self.into();

        dirs_and_file_name.push_dir(part);
        Self::Parts(dirs_and_file_name)
    }

    /// Push a bunch of parts as directories in one go.
    fn push_all_dirs<'a>(self, parts: impl AsRef<[&'a str]>) -> Self {
        let mut dirs_and_file_name: DirsAndFileName = self.into();

        dirs_and_file_name.push_all_dirs(parts);

        Self::Parts(dirs_and_file_name)
    }

    /// Add a `PathPart` to the end of the path's directories.
    fn push_part_as_dir(self, part: &PathPart) -> Self {
        let mut dirs_and_file_name: DirsAndFileName = self.into();

        dirs_and_file_name.push_part_as_dir(part);
        Self::Parts(dirs_and_file_name)
    }

    /// Set the file name of this path
    fn set_file_name(self, part: impl Into<String>) -> Self {
        let part = part.into();
        let mut dirs_and_file_name: DirsAndFileName = self.into();

        dirs_and_file_name.file_name = Some((&*part).into());
        Self::Parts(dirs_and_file_name)
    }
}

impl PartialEq for PathRepresentation {
    fn eq(&self, other: &Self) -> bool {
        use PathRepresentation::*;
        match (self, other) {
            (Parts(self_parts), Parts(other_parts)) => self_parts == other_parts,
            (Parts(self_parts), _) => {
                let other_parts: DirsAndFileName = other.to_owned().into();
                *self_parts == other_parts
            }
            (_, Parts(other_parts)) => {
                let self_parts: DirsAndFileName = self.to_owned().into();
                self_parts == *other_parts
            }
            _ => {
                let self_parts: DirsAndFileName = self.to_owned().into();
                let other_parts: DirsAndFileName = other.to_owned().into();
                self_parts == other_parts
            }
        }
    }
}

/// The delimiter to separate object namespaces, creating a directory structure.
pub const DELIMITER: &str = "/";

/// Converts `ObjectStorePath`s to `String`s that are appropriate for use as
/// locations in cloud storage.
#[derive(Debug, Clone, Copy)]
pub struct CloudConverter {}

impl CloudConverter {
    /// Creates a cloud storage location by joining this `ObjectStorePath`'s
    /// Creates a cloud storage location by joining this `CloudPath`'s
    /// parts with `DELIMITER`
    pub fn convert(object_store_path: &ObjectStorePath) -> String {
        match &object_store_path.inner {
            PathRepresentation::RawCloud(path) => path.to_owned(),
            PathRepresentation::RawPathBuf(_path) => {
                todo!("convert");
            }
            PathRepresentation::Parts(dirs_and_file_name) => {
                let mut path = dirs_and_file_name
                    .directories
                    .iter()
                    .map(PathPart::encoded)
                    .join(DELIMITER);

                if !path.is_empty() {
                    path.push_str(DELIMITER);
                }
                if let Some(file_name) = &dirs_and_file_name.file_name {
                    path.push_str(file_name.encoded());
                }
                path
            }
        }
    }
}

/// Converts `ObjectStorePath`s to `String`s that are appropriate for use as
/// locations in filesystem storage.
#[derive(Debug, Clone, Copy)]
pub struct FileConverter {}

impl FileConverter {
    /// Creates a filesystem `PathBuf` location by using the standard library's
    /// `PathBuf` building implementation appropriate for the current
    /// platform.
    pub fn convert(object_store_path: &ObjectStorePath) -> PathBuf {
        match &object_store_path.inner {
            PathRepresentation::RawCloud(_path) => {
                todo!("convert");
            }
            PathRepresentation::RawPathBuf(path) => path.to_owned(),
            PathRepresentation::Parts(dirs_and_file_name) => {
                let mut path: PathBuf = dirs_and_file_name
                    .directories
                    .iter()
                    .map(PathPart::encoded)
                    .collect();
                if let Some(file_name) = &dirs_and_file_name.file_name {
                    path.push(file_name.encoded());
                }
                path
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Invariants to maintain/document/test:
    //
    // - always ends in DELIMITER if it's a directory. If it's the end object, it
    //   should have some sort of file extension like .parquet, .json, or .segment
    // - does not contain unencoded DELIMITER
    // - for file paths: does not escape root dir
    // - for object storage: looks like directories
    // - Paths that come from object stores directly don't need to be
    //   parsed/validated
    // - Within a process, the same backing store will always be used
    //

    #[test]
    fn prefix_matches() {
        let mut haystack = ObjectStorePath::default();
        haystack.push_all_dirs(&["foo/bar", "baz%2Ftest", "something"]);

        // self starts with self
        assert!(
            haystack.prefix_matches(&haystack),
            "{:?} should have started with {:?}",
            haystack,
            haystack
        );

        // a longer prefix doesn't match
        let mut needle = haystack.clone();
        needle.push_dir("longer now");
        assert!(
            !haystack.prefix_matches(&needle),
            "{:?} shouldn't have started with {:?}",
            haystack,
            needle
        );

        // one dir prefix matches
        let mut needle = ObjectStorePath::default();
        needle.push_dir("foo/bar");
        assert!(
            haystack.prefix_matches(&needle),
            "{:?} should have started with {:?}",
            haystack,
            needle
        );

        // two dir prefix matches
        needle.push_dir("baz%2Ftest");
        assert!(
            haystack.prefix_matches(&needle),
            "{:?} should have started with {:?}",
            haystack,
            needle
        );

        // partial dir prefix matches
        let mut needle = ObjectStorePath::default();
        needle.push_dir("f");
        assert!(
            haystack.prefix_matches(&needle),
            "{:?} should have started with {:?}",
            haystack,
            needle
        );

        // one dir and one partial dir matches
        let mut needle = ObjectStorePath::default();
        needle.push_all_dirs(&["foo/bar", "baz"]);
        assert!(
            haystack.prefix_matches(&needle),
            "{:?} should have started with {:?}",
            haystack,
            needle
        );
    }

    #[test]
    fn prefix_matches_with_file_name() {
        let mut haystack = ObjectStorePath::default();
        haystack.push_all_dirs(&["foo/bar", "baz%2Ftest", "something"]);

        let mut needle = haystack.clone();

        // All directories match and file name is a prefix
        haystack.set_file_name("foo.segment");
        needle.set_file_name("foo");

        assert!(
            haystack.prefix_matches(&needle),
            "{:?} should have started with {:?}",
            haystack,
            needle
        );

        // All directories match but file name is not a prefix
        needle.set_file_name("e");

        assert!(
            !haystack.prefix_matches(&needle),
            "{:?} should not have started with {:?}",
            haystack,
            needle
        );

        // Not all directories match; file name is a prefix of the next directory; this
        // matches
        let mut needle = ObjectStorePath::default();
        needle.push_all_dirs(&["foo/bar", "baz%2Ftest"]);
        needle.set_file_name("s");

        assert!(
            haystack.prefix_matches(&needle),
            "{:?} should have started with {:?}",
            haystack,
            needle
        );

        // Not all directories match; file name is NOT a prefix of the next directory;
        // no match
        needle.set_file_name("p");

        assert!(
            !haystack.prefix_matches(&needle),
            "{:?} should not have started with {:?}",
            haystack,
            needle
        );
    }
}
