use std::future::Future;
use std::time::{Duration, SystemTime};

use uuid::Uuid;

use crate::fuse::{DirEntry, FileAttr, FileType, Request};

/// Errors produced by a filesystem backend.
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("not found")]
    NotFound,

    #[error("already exists")]
    AlreadyExists,

    #[error("not a directory")]
    NotADirectory,

    #[error("is a directory")]
    IsADirectory,

    #[error("directory not empty")]
    DirectoryNotEmpty,

    #[error("permission denied")]
    PermissionDenied,

    #[error("read-only filesystem")]
    ReadOnly,

    #[error("invalid argument")]
    InvalidArgument,

    #[error("name too long")]
    NameTooLong,

    #[error("no space left")]
    NoSpace,

    #[error("invalid cross-device link")]
    InvalidCrossDevice,

    #[error("operation not supported")]
    NotSupported,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type FsResult<T> = Result<T, FsError>;

/// Minimal filesystem backend.
///
/// Identity is always a UUID v7 (`file_id`). The FUSE layer maps it to a
/// `u64` inode via SipHash when a binary mounts the filesystem.
///
/// Names are plain `str` / `String` (UTF-8).
pub trait FsBackend: Send + Sync {
    // ---- identity / lookup ----

    fn lookup(
        &self,
        req: &Request,
        parent: Uuid,
        name: &str,
    ) -> impl Future<Output = FsResult<(FileAttr, Duration)>> + Send;

    fn getattr(
        &self,
        req: &Request,
        id: Uuid,
    ) -> impl Future<Output = FsResult<(FileAttr, Duration)>> + Send;

    // ---- directory ----

    /// Return all entries in the directory.
    /// The FUSE adapter turns this into an offset-based stream.
    fn readdir(
        &self,
        req: &Request,
        id: Uuid,
    ) -> impl Future<Output = FsResult<Vec<DirEntry>>> + Send;

    // ---- content ----

    fn read(
        &self,
        req: &Request,
        id: Uuid,
        offset: u64,
        size: u32,
    ) -> impl Future<Output = FsResult<Vec<u8>>> + Send;

    fn write(
        &self,
        req: &Request,
        id: Uuid,
        offset: u64,
        data: &[u8],
    ) -> impl Future<Output = FsResult<u32>> + Send;

    // ---- create ----

    fn create_file(
        &self,
        req: &Request,
        parent: Uuid,
        name: &str,
        mode: u32,
    ) -> impl Future<Output = FsResult<(FileAttr, Duration)>> + Send;

    fn mkdir(
        &self,
        req: &Request,
        parent: Uuid,
        name: &str,
        mode: u32,
    ) -> impl Future<Output = FsResult<(FileAttr, Duration)>> + Send;

    fn symlink(
        &self,
        req: &Request,
        parent: Uuid,
        name: &str,
        target: &str,
    ) -> impl Future<Output = FsResult<(FileAttr, Duration)>> + Send;

    fn mknod(
        &self,
        req: &Request,
        parent: Uuid,
        name: &str,
        kind: FileType,
        mode: u32,
        rdev: u32,
    ) -> impl Future<Output = FsResult<(FileAttr, Duration)>> + Send;

    // ---- remove ----

    fn unlink(
        &self,
        req: &Request,
        parent: Uuid,
        name: &str,
    ) -> impl Future<Output = FsResult<()>> + Send;

    fn rmdir(
        &self,
        req: &Request,
        parent: Uuid,
        name: &str,
    ) -> impl Future<Output = FsResult<()>> + Send;

    // ---- rename / link ----

    fn rename(
        &self,
        req: &Request,
        parent: Uuid,
        name: &str,
        newparent: Uuid,
        newname: &str,
        flags: u32,
    ) -> impl Future<Output = FsResult<()>> + Send;

    fn link(
        &self,
        req: &Request,
        id: Uuid,
        newparent: Uuid,
        newname: &str,
    ) -> impl Future<Output = FsResult<(FileAttr, Duration)>> + Send;

    // ---- symlink ----

    fn readlink(&self, req: &Request, id: Uuid) -> impl Future<Output = FsResult<Vec<u8>>> + Send;

    // ---- attributes ----

    fn setattr(
        &self,
        req: &Request,
        id: Uuid,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<SystemTime>,
        mtime: Option<SystemTime>,
        ctime: Option<SystemTime>,
    ) -> impl Future<Output = FsResult<(FileAttr, Duration)>> + Send;

    // ---- filesystem ----

    fn statfs(
        &self,
        req: &Request,
        id: Uuid,
    ) -> impl Future<Output = FsResult<libc::statvfs>> + Send;
}
