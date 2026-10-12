//! Safe Rust equivalents of the metadata portion of Unix `<sys/stat.h>`.
//!
//! [`stat`] follows symbolic links, [`lstat`] inspects the link itself, and
//! [`fstat`] queries an open [`File`], including after it has been unlinked.
//! Queries return [`io::Result<Stat>`] rather than writing through an output
//! pointer or setting `errno`. All fields come from
//! [`std::os::unix::fs::MetadataExt`].
//!
//! Path arguments accept [`ByteView`], including strings, byte slices, Unix
//! paths, and [`ZStr`]. Only bytes before the first NUL or view end are used;
//! non-UTF-8 paths are supported without conversion or allocation.
//! The [`c`] module provides
//! output parameters of type `&mut libc::stat`, integer results, and `errno`.
//!
//! ```no_run
//! use xj_cstat::{stat, S_ISREG, ZString};
//!
//! let info = stat(&ZString::new("Cargo.toml"))?;
//! assert!(S_ISREG(info.st_mode));
//! println!("{} bytes", info.st_size);
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! This is a Unix-only Rust API, not a C ABI. Field widths match `MetadataExt`,
//! rather than platform-specific C typedefs; in particular, sizes and block
//! counts are `u64`. Permission changes, creation, and timestamp setters from
//! `<sys/stat.h>` are outside this metadata API.

#![cfg(unix)]
#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;

pub mod c;
mod mode;
pub use mode::*;
/// Shared borrowed byte inputs, including Unix OS paths.
pub use xj_bytes::ByteView;
pub use xj_zstr::{ZStr, ZString};

/// A Unix timestamp, with seconds relative to the Unix epoch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Timespec {
    /// Whole seconds since the Unix epoch; negative values predate the epoch.
    pub tv_sec: i64,
    /// Nanoseconds past `tv_sec` (normally in `0..1_000_000_000`).
    pub tv_nsec: i64,
}

/// A snapshot of file attributes, with C-style names and `MetadataExt` widths.
///
/// Timestamps are split into seconds and nanoseconds. [`Self::st_atim`],
/// [`Self::st_mtim`], and [`Self::st_ctim`] combine each pair into a [`Timespec`].
/// This struct does not have the layout of the platform's C `struct stat`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Stat {
    /// ID of the device containing the file.
    pub st_dev: u64,
    /// Inode number.
    pub st_ino: u64,
    /// File type, permission bits, and special mode bits.
    pub st_mode: u32,
    /// Number of hard links.
    pub st_nlink: u64,
    /// User ID of the owner.
    pub st_uid: u32,
    /// Group ID of the owner.
    pub st_gid: u32,
    /// Device ID represented by a device special file.
    pub st_rdev: u64,
    /// File size in bytes.
    pub st_size: u64,
    /// Last access time, in whole seconds since the Unix epoch.
    pub st_atime: i64,
    /// Nanoseconds past `st_atime`.
    pub st_atime_nsec: i64,
    /// Last data modification time, in whole seconds since the Unix epoch.
    pub st_mtime: i64,
    /// Nanoseconds past `st_mtime`.
    pub st_mtime_nsec: i64,
    /// Last status change time, in whole seconds since the Unix epoch.
    ///
    /// This is not the file's creation time.
    pub st_ctime: i64,
    /// Nanoseconds past `st_ctime`.
    pub st_ctime_nsec: i64,
    /// Preferred block size for filesystem I/O, in bytes.
    pub st_blksize: u64,
    /// Allocated block count, in units of 512 bytes, including for sparse files.
    pub st_blocks: u64,
}

impl Stat {
    /// Returns the last access timestamp, including nanoseconds.
    pub const fn st_atim(&self) -> Timespec {
        Timespec {
            tv_sec: self.st_atime,
            tv_nsec: self.st_atime_nsec,
        }
    }

    /// Returns the last data modification timestamp, including nanoseconds.
    pub const fn st_mtim(&self) -> Timespec {
        Timespec {
            tv_sec: self.st_mtime,
            tv_nsec: self.st_mtime_nsec,
        }
    }

    /// Returns the last status change timestamp, including nanoseconds.
    pub const fn st_ctim(&self) -> Timespec {
        Timespec {
            tv_sec: self.st_ctime,
            tv_nsec: self.st_ctime_nsec,
        }
    }
}

impl From<&Metadata> for Stat {
    fn from(metadata: &Metadata) -> Self {
        Self {
            st_dev: metadata.dev(),
            st_ino: metadata.ino(),
            st_mode: metadata.mode(),
            st_nlink: metadata.nlink(),
            st_uid: metadata.uid(),
            st_gid: metadata.gid(),
            st_rdev: metadata.rdev(),
            st_size: metadata.size(),
            st_atime: metadata.atime(),
            st_atime_nsec: metadata.atime_nsec(),
            st_mtime: metadata.mtime(),
            st_mtime_nsec: metadata.mtime_nsec(),
            st_ctime: metadata.ctime(),
            st_ctime_nsec: metadata.ctime_nsec(),
            st_blksize: metadata.blksize(),
            st_blocks: metadata.blocks(),
        }
    }
}

impl From<Metadata> for Stat {
    fn from(metadata: Metadata) -> Self {
        Self::from(&metadata)
    }
}

/// Queries attributes of `path`, following symbolic links.
///
/// Uses [`fs::metadata`]. OS errors are returned unchanged. An empty effective
/// path is an error; it is not interpreted as the current directory.
pub fn stat<B: ByteView + ?Sized>(path: &B) -> io::Result<Stat> {
    fs::metadata(OsStr::from_bytes(xj_bytes::c_bytes(path.view_bytes()))).map(Stat::from)
}

/// Queries attributes of `path` without following the final symbolic link.
///
/// Uses [`fs::symlink_metadata`]. A dangling symbolic link can be inspected;
/// symbolic links in intermediate path components are still followed.
pub fn lstat<B: ByteView + ?Sized>(path: &B) -> io::Result<Stat> {
    fs::symlink_metadata(OsStr::from_bytes(xj_bytes::c_bytes(path.view_bytes())))
        .map(Stat::from)
}

/// Queries an open file without resolving its path or changing its position.
///
/// Uses [`File::metadata`]. Borrows the file without closing or duplicating its
/// descriptor. The file may represent a directory or another file type.
pub fn fstat(file: &File) -> io::Result<Stat> {
    file.metadata().map(Stat::from)
}
