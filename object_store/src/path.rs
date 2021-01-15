//! This module contains code for abstracting object locations that work
//! across different backing implementations and platforms.

/// Paths that came from or are to be used in cloud-based object storage
pub mod cloud;
use cloud::CloudPath;

/// Paths that come from or are to be used in file-based object storage
pub mod file;
use file::FilePath;

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
pub trait Osp: std::fmt::Debug + Default + Clone + PartialEq + Eq + Send + Sync + 'static {
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

/// Defines which object stores use which path logic.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ObjectStorePath {
    /// Amazon storage
    AmazonS3(CloudPath),
    /// Local file system storage
    File(FilePath),
    /// GCP storage
    GoogleCloudStorage(CloudPath),
    /// In memory storage for testing
    InMemory(DirsAndFileName),
    /// Microsoft Azure Blob storage
    MicrosoftAzure(CloudPath),
}

impl Default for ObjectStorePath {
    fn default() -> Self {
        Self::InMemory(DirsAndFileName::default())
    }
}

impl Osp for ObjectStorePath {
    fn push_dir(&mut self, _dir: impl Into<String>) {
        todo!()
    }

    fn push_all_dirs<'a>(&mut self, _parts: impl AsRef<[&'a str]>) {
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

/// The delimiter to separate object namespaces, creating a directory structure.
pub const DELIMITER: &str = "/";
