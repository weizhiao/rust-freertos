use crate::ffi::{CStr, OsString};
use crate::fmt;
use crate::sys::fd::FileDesc;
use crate::io::{self, Error};
use crate::io::{BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::path::{Path, PathBuf};
use crate::sys::common::small_c_string::run_path_with_cstr;
use crate::sys::{cvt,cvt_r};
use crate::sys::time::SystemTime;
use crate::sys::unsupported;
use crate::mem;

use libc::{c_int, mode_t, stat as stat64};

pub use crate::sys_common::fs::{copy, try_exists};
//pub use crate::sys_common::fs::remove_dir_all;

#[derive(Debug)]
pub struct File(FileDesc);

#[derive(Clone)]
pub struct FileAttr{stat:stat64}

pub struct ReadDir(!);

pub struct DirEntry(!);

#[derive(Clone, Debug)]
pub struct OpenOptions {
    // generic
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
    // system-specific
    custom_flags: i32,
    mode: mode_t,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileTimes {}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FilePermissions {
    mode: mode_t,
}

impl PartialEq for FileType {
    fn eq(&self, other: &Self) -> bool {
        self.masked() == other.masked()
    }
}

#[derive(Copy, Clone, Eq, Debug)]
pub struct FileType {
    mode: mode_t,
}

#[derive(Debug)]
pub struct DirBuilder {
    mode: mode_t,
}

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.stat.st_size as u64
    }

    fn from_stat(stat: stat64) -> Self {
        Self { stat }
    }

    pub fn perm(&self) -> FilePermissions {
        FilePermissions { mode: (self.stat.st_mode as mode_t) }
    }

    pub fn file_type(&self) -> FileType {
        FileType { mode: self.stat.st_mode as mode_t }
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        Err(io::const_io_error!(
            io::ErrorKind::Unsupported,
            "modified time is not available on this platform \
                            currently",
        ))
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        Err(io::const_io_error!(
            io::ErrorKind::Unsupported,
            "accessed time is not available on this platform \
                            currently",
        ))
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        Err(io::const_io_error!(
            io::ErrorKind::Unsupported,
            "creation time is not available on this platform \
                            currently",
        ))
    }
}

impl FilePermissions {
    pub fn readonly(&self) -> bool {
        // check if any class (owner, group, others) has write permission
        self.mode & 0o222 == 0
    }

    pub fn set_readonly(&mut self, readonly: bool) {
        if readonly {
            // remove write permission for all classes; equivalent to `chmod a-w <file>`
            self.mode &= !0o222;
        } else {
            // add write permission for all classes; equivalent to `chmod a+w <file>`
            self.mode |= 0o222;
        }
    }
}

impl FileTimes {
    pub fn set_accessed(&mut self, _t: SystemTime) {}
    pub fn set_modified(&mut self, _t: SystemTime) {}
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.is(libc::S_IFDIR)
    }
    pub fn is_file(&self) -> bool {
        self.is(libc::S_IFREG)
    }
    pub fn is_symlink(&self) -> bool {
        self.is(libc::S_IFLNK)
    }

    pub fn is(&self, mode: mode_t) -> bool {
        self.masked() == mode
    }

    fn masked(&self) -> mode_t {
        self.mode & libc::S_IFMT
    }
}

impl core::hash::Hash for FileType {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.masked().hash(state);
    }
}

impl fmt::Debug for ReadDir {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0
    }
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<io::Result<DirEntry>> {
        self.0
    }
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.0
    }

    pub fn file_name(&self) -> OsString {
        self.0
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        self.0
    }

    pub fn file_type(&self) -> io::Result<FileType> {
        self.0
    }
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions {
            // generic
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
            // system-specific
            custom_flags: 0,
            mode: 0o666,
        }
    }

    pub fn read(&mut self, read: bool) {
        self.read = read;
    }
    pub fn write(&mut self, write: bool) {
        self.write = write;
    }
    pub fn append(&mut self, append: bool) {
        self.append = append;
    }
    pub fn truncate(&mut self, truncate: bool) {
        self.truncate = truncate;
    }
    pub fn create(&mut self, create: bool) {
        self.create = create;
    }
    pub fn create_new(&mut self, create_new: bool) {
        self.create_new = create_new;
    }

    fn get_access_mode(&self) -> io::Result<c_int> {
        match (self.read, self.write, self.append) {
            (true, false, false) => Ok(libc::O_RDONLY),
            (false, true, false) => Ok(libc::O_WRONLY),
            (true, true, false) => Ok(libc::O_RDWR),
            (false, _, true) => Ok(libc::O_WRONLY | libc::O_APPEND),
            (true, _, true) => Ok(libc::O_RDWR | libc::O_APPEND),
            (false, false, false) => Err(Error::from_raw_os_error(libc::EINVAL)),
        }
    }

    fn get_creation_mode(&self) -> io::Result<c_int> {
        match (self.write, self.append) {
            (true, false) => {}
            (false, false) => {
                if self.truncate || self.create || self.create_new {
                    return Err(Error::from_raw_os_error(libc::EINVAL));
                }
            }
            (_, true) => {
                if self.truncate && !self.create_new {
                    return Err(Error::from_raw_os_error(libc::EINVAL));
                }
            }
        }

        Ok(match (self.create, self.truncate, self.create_new) {
            (false, false, false) => 0,
            (true, false, false) => libc::O_CREAT,
            (false, true, false) => libc::O_TRUNC,
            (true, true, false) => libc::O_CREAT | libc::O_TRUNC,
            (_, _, true) => libc::O_CREAT | libc::O_EXCL,
        })
    }
}

impl File {
    pub fn open(path: &Path, opts: &OpenOptions) -> io::Result<File> {
        run_path_with_cstr(path, |path| File::open_c(path, opts))
    }

    pub fn open_c(path: &CStr, opts: &OpenOptions) -> io::Result<File> {
        let flags = libc::O_CLOEXEC
            | opts.get_access_mode()?
            | opts.get_creation_mode()?
            | (opts.custom_flags as c_int & !libc::O_ACCMODE);
        let fd = cvt_r(|| unsafe { libc::open(path.as_ptr(), flags, opts.mode as c_int) })?;
        Ok(File(FileDesc::new(fd as i32)))
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        let fd = &self.0;
        let mut stat: stat64 = unsafe { mem::zeroed() };
        cvt(unsafe { libc::fstat(fd.raw(), &mut stat) })?;
        Ok(FileAttr::from_stat(stat))
    }

    pub fn fsync(&self) -> io::Result<()> {
        Err(Error::from_raw_os_error(22))
    }

    pub fn datasync(&self) -> io::Result<()> {
        self.fsync()
    }

    pub fn truncate(&self, _size: u64) -> io::Result<()> {
        Err(Error::from_raw_os_error(22))
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        crate::io::default_read_vectored(|buf| self.read(buf), bufs)
    }

    #[inline]
    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_buf(&self, cursor: BorrowedCursor<'_>) -> io::Result<()> {
        crate::io::default_read_buf(|buf| self.read(buf), cursor)
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        crate::io::default_write_vectored(|buf| self.write(buf), bufs)
    }

    #[inline]
    pub fn is_write_vectored(&self) -> bool {
        false
    }

    pub fn flush(&self) -> io::Result<()> {
        Ok(())
    }

    pub fn seek(&self, pos: SeekFrom) -> io::Result<u64> {
        let (whence, pos) = match pos {
            SeekFrom::Start(off) => (libc::SEEK_SET, off as i64),
            SeekFrom::End(off) => (libc::SEEK_END, off),
            SeekFrom::Current(off) => (libc::SEEK_CUR, off),
        };
        let n = cvt(unsafe { libc::lseek(self.0.raw(), pos as libc::off_t, whence) })?;
        Ok(n as u64)
    }

    pub fn duplicate(&self) -> io::Result<File> {
        Err(Error::from_raw_os_error(22))
    }

    pub fn set_permissions(&self, _perm: FilePermissions) -> io::Result<()> {
        Err(Error::from_raw_os_error(22))
    }

    pub fn set_times(&self, _times: FileTimes) -> io::Result<()> {
        Err(Error::from_raw_os_error(22))
    }
}

impl DirBuilder {
    pub fn new() -> DirBuilder {
        DirBuilder { mode: 0o777 }
    }

    pub fn mkdir(&self, p: &Path) -> io::Result<()> {
        run_path_with_cstr(p, |p| cvt(unsafe { libc::mkdir(p.as_ptr(), self.mode) }).map(|_| ()))
    }
}

pub fn readdir(_p: &Path) -> io::Result<ReadDir> {
    unsupported()
}

pub fn unlink(path: &Path) -> io::Result<()> {
    run_path_with_cstr(path, |path| cvt(unsafe { libc::unlink(path.as_ptr()) }).map(|_| ()))
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    run_path_with_cstr(old, |old| {
        run_path_with_cstr(new, |new| {
            cvt(unsafe { libc::rename(old.as_ptr(), new.as_ptr()) }).map(|_| ())
        })
    })
}

pub fn set_perm(_p: &Path, _perm: FilePermissions) -> io::Result<()> {
    unsupported()
}

pub fn rmdir(p: &Path) -> io::Result<()> {
    run_path_with_cstr(p, |p| cvt(unsafe { libc::rmdir(p.as_ptr()) }).map(|_| ()))
}

pub fn remove_dir_all(_path: &Path) -> io::Result<()> {
    //unsupported()
    Ok(())
}

pub fn readlink(_p: &Path) -> io::Result<PathBuf> {
    unsupported()
}

pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> {
    unsupported()
}

pub fn link(_original: &Path, _link: &Path) -> io::Result<()> {
    unsupported()
}

pub fn stat(p: &Path) -> io::Result<FileAttr> {
    run_path_with_cstr(p, |p| {
        let mut stat: stat64 = unsafe { mem::zeroed() };
        cvt(unsafe { libc::stat(p.as_ptr(), &mut stat) })?;
        Ok(FileAttr::from_stat(stat))
    })
}

pub fn lstat(_p: &Path) -> io::Result<FileAttr> {
    unsupported()
}

pub fn canonicalize(_p: &Path) -> io::Result<PathBuf> {
    unsupported()
}
