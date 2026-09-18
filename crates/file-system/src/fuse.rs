use std::{
    ffi::{OsStr, OsString},
    future::Future,
    path::Path,
    time::{Duration, SystemTime},
};

use bitflags::bitflags;

pub type Inode = u64;
pub type FileHandle = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    NamedPipe,
    CharDevice,
    Directory,
    BlockDevice,
    RegularFile,
    Symlink,
    Socket,
}

#[derive(Debug, Clone)]
pub struct FileAttr {
    pub ino: Inode,
    pub size: u64,
    pub blocks: u64,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
    pub crtime: SystemTime,
    pub kind: FileType,
    pub perm: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u32,
    pub blksize: u32,
    pub flags: u32,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub ino: Inode,
    pub offset: i64,
    pub kind: FileType,
    pub name: OsString,
}

#[derive(Debug, Clone, Copy)]
pub struct Request {
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct OpenFlags: i32 {
        const RDONLY   = 0;
        const WRONLY   = 1;
        const RDWR     = 2;
        const CREAT    = 0o100;
        const EXCL     = 0o200;
        const TRUNC    = 0o1000;
        const APPEND   = 0o2000;
        const NONBLOCK = 0o4000;
    }
}

pub type FuseResult<T> = Result<T, i32>;

/// Async FUSE low-level filesystem.
///
/// Methods that cannot be expressed purely in terms of other methods are
/// required. Defaults exist only where they are true compositions.
pub trait FuseFilesystem: Send + Sync {
    // ------------------------------------------------------------------
    // Lifecycle – pure no side-effect is acceptable for these two
    // ------------------------------------------------------------------
    fn init(&mut self, _req: &Request) -> impl Future<Output = FuseResult<()>> + Send {
        async { Ok(()) }
    }

    fn destroy(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }

    // ------------------------------------------------------------------
    // Required core methods (no default possible without inventing state)
    // ------------------------------------------------------------------

    fn lookup(
        &self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
    ) -> impl Future<Output = FuseResult<(FileAttr, Duration)>> + Send;

    fn getattr(
        &self,
        req: &Request,
        ino: Inode,
        fh: Option<FileHandle>,
    ) -> impl Future<Output = FuseResult<(FileAttr, Duration)>> + Send;

    fn readdir(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        offset: i64,
    ) -> impl Future<Output = FuseResult<Vec<DirEntry>>> + Send;

    fn read(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        offset: u64,
        size: u32,
    ) -> impl Future<Output = FuseResult<Vec<u8>>> + Send;

    // ------------------------------------------------------------------
    // Required mutation / open methods
    // (cannot be derived without knowing the concrete storage)
    // ------------------------------------------------------------------

    fn setattr(
        &self,
        req: &Request,
        ino: Inode,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<SystemTime>,
        mtime: Option<SystemTime>,
        ctime: Option<SystemTime>,
        fh: Option<FileHandle>,
    ) -> impl Future<Output = FuseResult<(FileAttr, Duration)>> + Send;

    fn open(
        &self,
        req: &Request,
        ino: Inode,
        flags: OpenFlags,
    ) -> impl Future<Output = FuseResult<(FileHandle, OpenFlags)>> + Send;

    fn write(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        flags: OpenFlags,
    ) -> impl Future<Output = FuseResult<u32>> + Send;

    fn mknod(
        &self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
    ) -> impl Future<Output = FuseResult<(FileAttr, Duration)>> + Send;

    fn unlink(
        &self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    fn rmdir(
        &self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    fn rename(
        &self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
        newparent: Inode,
        newname: &OsStr,
        flags: u32,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    fn link(
        &self,
        req: &Request,
        ino: Inode,
        newparent: Inode,
        newname: &OsStr,
    ) -> impl Future<Output = FuseResult<(FileAttr, Duration)>> + Send;

    fn readlink(
        &self,
        req: &Request,
        ino: Inode,
    ) -> impl Future<Output = FuseResult<Vec<u8>>> + Send;

    fn symlink(
        &self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
        target: &Path,
    ) -> impl Future<Output = FuseResult<(FileAttr, Duration)>> + Send;

    // ------------------------------------------------------------------
    // Defaults that are real compositions of required methods
    // ------------------------------------------------------------------

    /// create = mknod + open
    fn create(
        &self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: OpenFlags,
    ) -> impl Future<Output = FuseResult<(FileAttr, FileHandle, OpenFlags, Duration)>> + Send {
        async move {
            let (attr, timeout) = self.mknod(req, parent, name, mode, umask, 0).await?;
            let (fh, open_flags) = self.open(req, attr.ino, flags).await?;
            Ok((attr, fh, open_flags, timeout))
        }
    }

    /// mkdir = mknod with directory type
    fn mkdir(
        &self,
        req: &Request,
        parent: Inode,
        name: &OsStr,
        mode: u32,
        umask: u32,
    ) -> impl Future<Output = FuseResult<(FileAttr, Duration)>> + Send {
        async move {
            self.mknod(req, parent, name, mode | libc::S_IFDIR, umask, 0)
                .await
        }
    }

    // ------------------------------------------------------------------
    // Directory open/close – must still be supplied by implementor
    // (they manage the FileHandle lifetime)
    // ------------------------------------------------------------------

    fn opendir(
        &self,
        req: &Request,
        ino: Inode,
        flags: OpenFlags,
    ) -> impl Future<Output = FuseResult<(FileHandle, OpenFlags)>> + Send;

    fn releasedir(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        flags: OpenFlags,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    fn fsyncdir(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        datasync: bool,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    // ------------------------------------------------------------------
    // File close / sync – must be supplied (they manage resources)
    // ------------------------------------------------------------------

    fn flush(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        lock_owner: u64,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    fn release(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        flags: OpenFlags,
        lock_owner: Option<u64>,
        flush: bool,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    fn fsync(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        datasync: bool,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    // ------------------------------------------------------------------
    // forget – pure bookkeeping, no side-effect default is fine
    // ------------------------------------------------------------------

    fn forget(
        &self,
        _req: &Request,
        _ino: Inode,
        _nlookup: u64,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    // ------------------------------------------------------------------
    // Remaining methods that cannot be derived – required
    // ------------------------------------------------------------------

    fn statfs(
        &self,
        req: &Request,
        ino: Inode,
    ) -> impl Future<Output = FuseResult<libc::statvfs>> + Send;

    fn access(
        &self,
        req: &Request,
        ino: Inode,
        mask: i32,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    fn fallocate(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        offset: u64,
        length: u64,
        mode: i32,
    ) -> impl Future<Output = FuseResult<()>> + Send;

    fn lseek(
        &self,
        req: &Request,
        ino: Inode,
        fh: FileHandle,
        offset: i64,
        whence: i32,
    ) -> impl Future<Output = FuseResult<i64>> + Send;
}
