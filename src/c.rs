//! Libc-style metadata queries with safe references.
//!
//! These functions take an initialized `&mut libc::stat` (or, where available,
//! `&mut libc::stat64`), return `0` on success, and return `-1` with thread-local
//! `errno` set on failure. Path queries accept [`ByteView`] instead of taking raw
//! pointers. [`fstat_f`] borrows a [`File`], while [`fstat`] accepts a C integer
//! file descriptor. The corresponding large-file functions use `64` suffixes.
//!
//! The path and `File` queries use the crate's `MetadataExt` wrappers. The
//! integer-descriptor functions call libc directly because Rust cannot soundly
//! create a borrowed [`File`] from an arbitrary integer. Their small unsafe
//! blocks are contained here; the public functions remain safe because the
//! kernel validates each descriptor and each output parameter is a Rust
//! reference.
//!
//! For metadata-backed queries, all fields represented by [`Stat`] are copied
//! with checked conversions; a value that cannot fit the platform's C field
//! produces `EOVERFLOW`. Platform-specific fields outside `MetadataExt` (such
//! as birth time, flags, and reserved fields) are retained from the supplied
//! output. Integer-descriptor queries let libc populate the native structure.
//! Every function leaves its output unchanged on failure.
//!
//! ```no_run
//! use xj_cstat::{c, ZStr};
//!
//! fn inspect(path: &ZStr, output: &mut libc::stat) -> std::io::Result<()> {
//!     if c::stat(path, output) == -1 {
//!         return Err(std::io::Error::last_os_error());
//!     }
//!     println!("{} bytes", output.st_size);
//!     Ok(())
//! }
//! ```

use std::fs::File;
use std::io;

use crate::{ByteView, Stat};

// Expose the mode constants and predicates alongside the C-style queries.
pub use crate::mode::*;

/// Queries `path`, following symbolic links, and fills `buf` on success.
///
/// Returns `0` on success or `-1` with `errno` set on failure. Failure leaves
/// `buf` unchanged. Only fields provided by `MetadataExt` are updated.
pub fn stat<B: ByteView + ?Sized>(path: &B, buf: &mut libc::stat) -> libc::c_int {
    finish(crate::stat(path), buf)
}

/// Queries `path` without following its final symbolic link.
///
/// Returns `0` on success or `-1` with `errno` set on failure. Failure leaves
/// `buf` unchanged. Only fields provided by `MetadataExt` are updated.
pub fn lstat<B: ByteView + ?Sized>(path: &B, buf: &mut libc::stat) -> libc::c_int {
    finish(crate::lstat(path), buf)
}

/// Queries an open file and fills `buf` on success.
///
/// Borrows the file without closing or duplicating its descriptor. Returns `0`
/// on success or `-1` with `errno` set on failure. Failure leaves `buf` unchanged.
/// Only fields provided by `MetadataExt` are updated.
pub fn fstat_f(file: &File, buf: &mut libc::stat) -> libc::c_int {
    finish(crate::fstat(file), buf)
}

/// Queries the open file descriptor `fd` and fills `buf` on success.
///
/// This closely matches libc's `fstat`, except the output pointer is replaced
/// by a safe mutable reference. Returns `0` on success or `-1` with `errno` set
/// by libc on failure. The descriptor is neither duplicated nor closed, and
/// the file position is unchanged. Failure leaves `buf` unchanged.
#[allow(unsafe_code)]
pub fn fstat(fd: libc::c_int, buf: &mut libc::stat) -> libc::c_int {
    let mut output = *buf;
    // SAFETY: `output` is a valid, initialized `libc::stat` with exclusive
    // access for the duration of the call. libc accepts every `c_int` value;
    // the kernel reports an invalid or closed descriptor as `EBADF`.
    let result = unsafe { libc::fstat(fd, &mut output) };
    if result == 0 {
        *buf = output;
    }
    result
}

/// Queries `path` with large-file semantics, following symbolic links.
///
/// Returns `0` on success or `-1` with `errno` set on failure. Failure leaves
/// `buf` unchanged. This interface is available on GNU/Linux targets that
/// expose `libc::stat64`.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn stat64<B: ByteView + ?Sized>(path: &B, buf: &mut libc::stat64) -> libc::c_int {
    finish64(crate::stat(path), buf)
}

/// Queries `path` with large-file semantics without following its final link.
///
/// Returns `0` on success or `-1` with `errno` set on failure. Failure leaves
/// `buf` unchanged. This interface is available on GNU/Linux targets that
/// expose `libc::stat64`.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn lstat64<B: ByteView + ?Sized>(path: &B, buf: &mut libc::stat64) -> libc::c_int {
    finish64(crate::lstat(path), buf)
}

/// Queries an open [`File`] and fills a large-file stat buffer.
///
/// The file is borrowed without closing or duplicating its descriptor. Returns
/// `0` on success or `-1` with `errno` set on failure. Failure leaves `buf`
/// unchanged. This interface is available on GNU/Linux targets.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn fstat64_f(file: &File, buf: &mut libc::stat64) -> libc::c_int {
    finish64(crate::fstat(file), buf)
}

/// Queries an integer descriptor and fills a large-file stat buffer.
///
/// This closely matches libc's `fstat64`, except the output pointer is replaced
/// by a safe mutable reference. The descriptor is neither duplicated nor
/// closed. Returns `0` on success or `-1` with `errno` set by libc on failure;
/// failure leaves `buf` unchanged. This interface is available on GNU/Linux.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(unsafe_code)]
pub fn fstat64(fd: libc::c_int, buf: &mut libc::stat64) -> libc::c_int {
    let mut output = *buf;
    // SAFETY: `output` is a valid, initialized `libc::stat64` with exclusive
    // access during the call. libc accepts any `c_int`; an invalid or closed
    // descriptor is reported as `EBADF`.
    let result = unsafe { libc::fstat64(fd, &mut output) };
    if result == 0 {
        *buf = output;
    }
    result
}

fn finish(result: io::Result<Stat>, buf: &mut libc::stat) -> libc::c_int {
    let result = result
        .map_err(|error| error.raw_os_error().unwrap_or(libc::EIO))
        .and_then(|info| write_fields(info, buf));
    match result {
        Ok(()) => 0,
        Err(code) => {
            errno::set_errno(errno::Errno(code));
            -1
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn finish64(result: io::Result<Stat>, buf: &mut libc::stat64) -> libc::c_int {
    let result = result
        .map_err(|error| error.raw_os_error().unwrap_or(libc::EIO))
        .and_then(|info| write_fields64(info, buf));
    match result {
        Ok(()) => 0,
        Err(code) => {
            errno::set_errno(errno::Errno(code));
            -1
        }
    }
}

fn checked<T, U: TryFrom<T>>(value: T) -> Result<U, libc::c_int> {
    U::try_from(value).map_err(|_| libc::EOVERFLOW)
}

macro_rules! assign_metadata_fields {
    ($converted:ident, $info:ident) => {
        $converted.st_dev = checked($info.st_dev)?;
        $converted.st_ino = checked($info.st_ino)?;
        $converted.st_mode = checked($info.st_mode)?;
        $converted.st_nlink = checked($info.st_nlink)?;
        $converted.st_uid = checked($info.st_uid)?;
        $converted.st_gid = checked($info.st_gid)?;
        $converted.st_rdev = checked($info.st_rdev)?;
        $converted.st_size = checked($info.st_size)?;
        $converted.st_atime = checked($info.st_atime)?;
        $converted.st_atime_nsec = checked($info.st_atime_nsec)?;
        $converted.st_mtime = checked($info.st_mtime)?;
        $converted.st_mtime_nsec = checked($info.st_mtime_nsec)?;
        $converted.st_ctime = checked($info.st_ctime)?;
        $converted.st_ctime_nsec = checked($info.st_ctime_nsec)?;
        $converted.st_blksize = checked($info.st_blksize)?;
        $converted.st_blocks = checked($info.st_blocks)?;
    };
}

fn write_fields(info: Stat, buf: &mut libc::stat) -> Result<(), libc::c_int> {
    // Start with a copy to preserve fields absent from MetadataExt and to avoid
    // partially overwriting the caller's output if a later conversion fails.
    let mut converted = *buf;
    assign_metadata_fields!(converted, info);
    *buf = converted;
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn write_fields64(info: Stat, buf: &mut libc::stat64) -> Result<(), libc::c_int> {
    let mut converted = *buf;
    assign_metadata_fields!(converted, info);
    *buf = converted;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::checked;

    #[test]
    fn c_field_conversions_check_overflow() {
        assert_eq!(checked::<_, libc::off_t>(u64::MAX), Err(libc::EOVERFLOW));
        assert_eq!(checked::<_, libc::blkcnt_t>(u64::MAX), Err(libc::EOVERFLOW));
        assert_eq!(checked::<_, u16>(u32::MAX), Err(libc::EOVERFLOW));
        assert_eq!(checked::<_, i32>(i64::MAX), Err(libc::EOVERFLOW));
        assert_eq!(checked::<_, libc::time_t>(-2_i64), Ok(-2));
    }
}
