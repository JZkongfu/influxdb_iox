use super::{DirsAndFileName, Osp, PathPart};

use std::{mem, path::PathBuf};

/// An object storage location suitable for passing to disk based object
/// storage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilePath {
    inner: FilePathRepresentation,
}

impl Osp for FilePath {
    fn set_file_name(&mut self, part: impl Into<String>) {
        self.inner = mem::take(&mut self.inner).set_file_name(part);
    }

    fn push_dir(&mut self, part: impl Into<String>) {
        self.inner = mem::take(&mut self.inner).push_dir(part);
    }

    fn push_all_dirs<'a>(&mut self, parts: impl AsRef<[&'a str]>) {
        self.inner = mem::take(&mut self.inner).push_all_dirs(parts);
    }

    fn display(&self) -> String {
        todo!()
    }
}

impl FilePath {
    /// Creates a file storage location from a `PathBuf` without parsing or
    /// allocating unless other methods are called on this instance that
    /// need it
    pub fn raw(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            inner: FilePathRepresentation::Raw(path),
        }
    }

    /// Creates a filesystem `PathBuf` location by using the standard library's
    /// `PathBuf` building implementation appropriate for the current
    /// platform.
    pub fn to_raw(&self) -> PathBuf {
        use FilePathRepresentation::*;

        match &self.inner {
            Raw(path) => path.to_owned(),
            Parsed(dirs_and_file_name) => {
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

impl From<FilePath> for DirsAndFileName {
    fn from(file_path: FilePath) -> Self {
        file_path.inner.into()
    }
}

impl From<DirsAndFileName> for FilePath {
    fn from(dirs_and_file_name: DirsAndFileName) -> Self {
        Self {
            inner: FilePathRepresentation::Parsed(dirs_and_file_name),
        }
    }
}

#[derive(Debug, Clone, Eq)]
enum FilePathRepresentation {
    Raw(PathBuf),
    Parsed(DirsAndFileName),
}

impl Default for FilePathRepresentation {
    fn default() -> Self {
        Self::Parsed(DirsAndFileName::default())
    }
}

impl PartialEq for FilePathRepresentation {
    fn eq(&self, other: &Self) -> bool {
        use FilePathRepresentation::*;
        match (self, other) {
            (Parsed(self_parts), Parsed(other_parts)) => self_parts == other_parts,
            (Parsed(self_parts), _) => {
                let other_parts: DirsAndFileName = other.to_owned().into();
                *self_parts == other_parts
            }
            (_, Parsed(other_parts)) => {
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

impl FilePathRepresentation {
    fn push_dir(self, part: impl Into<String>) -> Self {
        let mut dirs_and_file_name: DirsAndFileName = self.into();

        dirs_and_file_name.push_dir(part);
        Self::Parsed(dirs_and_file_name)
    }

    fn push_all_dirs<'a>(self, parts: impl AsRef<[&'a str]>) -> Self {
        let mut dirs_and_file_name: DirsAndFileName = self.into();

        dirs_and_file_name.push_all_dirs(parts);
        Self::Parsed(dirs_and_file_name)
    }

    fn set_file_name(self, part: impl Into<String>) -> Self {
        let mut dirs_and_file_name: DirsAndFileName = self.into();

        dirs_and_file_name.set_file_name(part);
        Self::Parsed(dirs_and_file_name)
    }
}

impl From<FilePathRepresentation> for DirsAndFileName {
    fn from(file_path_rep: FilePathRepresentation) -> Self {
        use FilePathRepresentation::*;

        match file_path_rep {
            Raw(path) => {
                let mut parts: Vec<PathPart> = path
                    .iter()
                    .flat_map(|s| s.to_os_string().into_string().map(PathPart))
                    .collect();

                let maybe_file_name = match parts.pop() {
                    Some(file)
                        if !file.encoded().starts_with('.')
                            && (file.encoded().ends_with(".json")
                                || file.encoded().ends_with(".parquet")
                                || file.encoded().ends_with(".segment")) =>
                    {
                        Some(file)
                    }
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
