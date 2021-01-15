use super::{PathPart, DELIMITER, DirsAndFileName, PathRepresentation, ObjectStorePath};

use itertools::Itertools;

use std::mem;

#[derive(Debug, Clone, Default)]
pub struct CloudPath {
    inner: CloudPathRepresentation,
}

impl CloudPath {
    pub(crate) fn raw(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            inner: CloudPathRepresentation::Raw(path),
        }
    }

    pub(crate) fn push_dir(&mut self, part: impl Into<String>) {
        self.inner = mem::take(&mut self.inner).push_dir(part);
    }
}

impl From<ObjectStorePath> for CloudPath {
    fn from(object_store_path: ObjectStorePath) -> Self {
        use PathRepresentation::*;

        let inner = match object_store_path.inner {
            RawCloud(path) => CloudPathRepresentation::Raw(path),
            RawPathBuf(_) => panic!("Invalid to convert PathBuf to CloudPath"),
            Parts(dirs_and_file_name) => CloudPathRepresentation::Parsed(dirs_and_file_name),
        };

        Self { inner }
    }
}

#[derive(Debug, Clone)]
enum CloudPathRepresentation {
    Raw(String),
    Parsed(DirsAndFileName),
}

impl Default for CloudPathRepresentation {
    fn default() -> Self {
        Self::Parsed(DirsAndFileName::default())
    }
}

impl CloudPathRepresentation {
    fn push_dir(self, part: impl Into<String>) -> Self {
        let mut dirs_and_file_name: DirsAndFileName = self.into();

        dirs_and_file_name.push_dir(part);
        Self::Parsed(dirs_and_file_name)
    }
}

impl From<CloudPathRepresentation> for DirsAndFileName {
    fn from(cloud_path_rep: CloudPathRepresentation) -> Self {
        use CloudPathRepresentation::*;

        match cloud_path_rep {
            Raw(path) => {
                let mut parts: Vec<PathPart> = path
                    .split_terminator(DELIMITER)
                    .map(|s| PathPart(s.to_string()))
                    .collect();
                let maybe_file_name = match parts.pop() {
                    Some(file) if file.encoded().contains('.') => Some(file),
                    Some(dir) => {
                        parts.push(dir);
                        None
                    }
                    None => None,
                };
                Self {
                    directories: parts,
                    file_name: maybe_file_name,
                }
            }
            Parsed(dirs_and_file_name) => dirs_and_file_name,
        }
    }
}

/// Converts `CloudPath`s to `String`s that are appropriate for use as
/// locations in cloud storage.
#[derive(Debug, Clone, Copy)]
pub struct CloudConverter {}

impl CloudConverter {
    /// Creates a cloud storage location by joining this `CloudPath`'s
    /// parts with `DELIMITER`
    pub fn convert(cloud_path: &CloudPath) -> String {
        use CloudPathRepresentation::*;

        match &cloud_path.inner {
            Raw(path) => path.to_owned(),
            Parsed(dirs_and_file_name) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_prefix_no_trailing_delimiter_or_file_name() {
        // Use case: a file named `test_file.json` exists in object storage and it
        // should be returned for a search on prefix `test`, so the prefix path
        // should not get a trailing delimiter automatically added
        let mut prefix = CloudPath::default();
        prefix.set_file_name("test");

        let converted = CloudConverter::convert(&prefix);
        assert_eq!(converted, "test");
    }

    #[test]
    fn cloud_prefix_with_trailing_delimiter() {
        // Use case: files exist in object storage named `foo/bar.json` and
        // `foo_test.json`. A search for the prefix `foo/` should return
        // `foo/bar.json` but not `foo_test.json'.
        let mut prefix = CloudPath::default();
        prefix.push_dir("test");

        let converted = CloudConverter::convert(&prefix);
        assert_eq!(converted, "test/");
    }

    #[test]
    fn push_encodes() {
        let mut location = CloudPath::default();
        location.push_dir("foo/bar");
        location.push_dir("baz%2Ftest");

        let converted = CloudConverter::convert(&location);
        assert_eq!(converted, "foo%2Fbar/baz%252Ftest/");
    }

    #[test]
    fn push_all_encodes() {
        let mut location = CloudPath::default();
        location.push_all_dirs(&["foo/bar", "baz%2Ftest"]);

        let converted = CloudConverter::convert(&location);
        assert_eq!(converted, "foo%2Fbar/baz%252Ftest/");
    }
}
