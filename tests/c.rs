#![cfg(unix)]

use std::fs::{self, File};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, symlink};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use xj_cstat::{ZStr, ZString, c};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "xj_cstat-c-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn zpath(path: &Path) -> ZString {
    ZString::new(path.as_os_str().as_bytes())
}

fn blank_stat() -> libc::stat {
    // SAFETY: libc::stat contains integer fields and padding. An all-zero value
    // is valid, and the tested API requires an initialized output value.
    unsafe { std::mem::zeroed() }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn blank_stat64() -> libc::stat64 {
    // SAFETY: libc::stat64 contains integer fields and padding. An all-zero
    // value is valid, and the API requires an initialized output value.
    unsafe { std::mem::zeroed() }
}

fn assert_fields_equal(actual: &libc::stat, expected: &fs::Metadata) {
    assert_eq!(actual.st_dev, expected.dev());
    assert_eq!(actual.st_ino, expected.ino());
    assert_eq!(actual.st_mode, expected.mode());
    assert_eq!(actual.st_nlink, expected.nlink());
    assert_eq!(actual.st_uid, expected.uid());
    assert_eq!(actual.st_gid, expected.gid());
    assert_eq!(actual.st_rdev, expected.rdev());
    assert_eq!(actual.st_size, expected.size() as libc::off_t);
    assert_eq!(actual.st_atime, expected.atime() as libc::time_t);
    assert_eq!(actual.st_atime_nsec, expected.atime_nsec());
    assert_eq!(actual.st_mtime, expected.mtime() as libc::time_t);
    assert_eq!(actual.st_mtime_nsec, expected.mtime_nsec());
    assert_eq!(actual.st_ctime, expected.ctime() as libc::time_t);
    assert_eq!(actual.st_ctime_nsec, expected.ctime_nsec());
    assert_eq!(actual.st_blksize, expected.blksize() as libc::blksize_t);
    assert_eq!(actual.st_blocks, expected.blocks() as libc::blkcnt_t);
}

fn assert_stat_fields_equal(actual: &libc::stat, expected: &libc::stat) {
    assert_eq!(actual.st_dev, expected.st_dev);
    assert_eq!(actual.st_ino, expected.st_ino);
    assert_eq!(actual.st_mode, expected.st_mode);
    assert_eq!(actual.st_nlink, expected.st_nlink);
    assert_eq!(actual.st_uid, expected.st_uid);
    assert_eq!(actual.st_gid, expected.st_gid);
    assert_eq!(actual.st_rdev, expected.st_rdev);
    assert_eq!(actual.st_size, expected.st_size);
    assert_eq!(actual.st_atime, expected.st_atime);
    assert_eq!(actual.st_atime_nsec, expected.st_atime_nsec);
    assert_eq!(actual.st_mtime, expected.st_mtime);
    assert_eq!(actual.st_mtime_nsec, expected.st_mtime_nsec);
    assert_eq!(actual.st_ctime, expected.st_ctime);
    assert_eq!(actual.st_ctime_nsec, expected.st_ctime_nsec);
    assert_eq!(actual.st_blksize, expected.st_blksize);
    assert_eq!(actual.st_blocks, expected.st_blocks);
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn assert_fields_equal64(actual: &libc::stat64, expected: &fs::Metadata) {
    assert_eq!(actual.st_dev, expected.dev());
    assert_eq!(actual.st_ino, expected.ino());
    assert_eq!(actual.st_mode, expected.mode());
    assert_eq!(actual.st_nlink, expected.nlink());
    assert_eq!(actual.st_uid, expected.uid());
    assert_eq!(actual.st_gid, expected.gid());
    assert_eq!(actual.st_rdev, expected.rdev());
    assert_eq!(actual.st_size, expected.size() as libc::off64_t);
    assert_eq!(actual.st_atime, expected.atime() as libc::time_t);
    assert_eq!(actual.st_atime_nsec, expected.atime_nsec());
    assert_eq!(actual.st_mtime, expected.mtime() as libc::time_t);
    assert_eq!(actual.st_mtime_nsec, expected.mtime_nsec());
    assert_eq!(actual.st_ctime, expected.ctime() as libc::time_t);
    assert_eq!(actual.st_ctime_nsec, expected.ctime_nsec());
    assert_eq!(actual.st_blksize, expected.blksize() as libc::blksize_t);
    assert_eq!(actual.st_blocks, expected.blocks() as libc::blkcnt64_t);
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn assert_stat64_fields_equal(actual: &libc::stat64, expected: &libc::stat64) {
    assert_eq!(actual.st_dev, expected.st_dev);
    assert_eq!(actual.st_ino, expected.st_ino);
    assert_eq!(actual.st_mode, expected.st_mode);
    assert_eq!(actual.st_nlink, expected.st_nlink);
    assert_eq!(actual.st_uid, expected.st_uid);
    assert_eq!(actual.st_gid, expected.st_gid);
    assert_eq!(actual.st_rdev, expected.st_rdev);
    assert_eq!(actual.st_size, expected.st_size);
    assert_eq!(actual.st_atime, expected.st_atime);
    assert_eq!(actual.st_atime_nsec, expected.st_atime_nsec);
    assert_eq!(actual.st_mtime, expected.st_mtime);
    assert_eq!(actual.st_mtime_nsec, expected.st_mtime_nsec);
    assert_eq!(actual.st_ctime, expected.st_ctime);
    assert_eq!(actual.st_ctime_nsec, expected.st_ctime_nsec);
    assert_eq!(actual.st_blksize, expected.st_blksize);
    assert_eq!(actual.st_blocks, expected.st_blocks);
}

#[test]
fn stat_accepts_borrowed_zstr_and_fills_libc_stat() -> io::Result<()> {
    let dir = TestDir::new()?;
    let path = dir.join("file");
    fs::write(&path, b"metadata")?;
    let expected = fs::metadata(&path)?;
    let owned = zpath(&path);
    let path: &ZStr = owned.as_z_str();
    let mut output = blank_stat();

    errno::set_errno(errno::Errno(libc::EDOM));
    assert_eq!(c::stat(path, &mut output), 0);
    assert_eq!(errno::errno(), errno::Errno(libc::EDOM));
    assert_fields_equal(&output, &expected);
    Ok(())
}

#[test]
fn lstat_does_not_follow_final_symlink() -> io::Result<()> {
    let dir = TestDir::new()?;
    fs::write(dir.join("target"), b"data")?;
    let link = dir.join("link");
    symlink("target", &link)?;
    let path = zpath(&link);
    let mut output = blank_stat();

    assert_eq!(c::lstat(&path, &mut output), 0);
    assert_fields_equal(&output, &fs::symlink_metadata(&link)?);
    assert_eq!(output.st_mode & libc::S_IFMT, libc::S_IFLNK);
    Ok(())
}

#[test]
fn fstat_f_queries_open_file() -> io::Result<()> {
    let dir = TestDir::new()?;
    let path = dir.join("file");
    fs::write(&path, b"open")?;
    let file = File::open(&path)?;
    fs::remove_file(path)?;
    let mut output = blank_stat();

    assert_eq!(c::fstat_f(&file, &mut output), 0);
    assert_fields_equal(&output, &file.metadata()?);
    assert_eq!(output.st_nlink, 0);
    Ok(())
}

#[test]
fn fstat_queries_integer_descriptor_without_taking_ownership() -> io::Result<()> {
    let dir = TestDir::new()?;
    let path = dir.join("file");
    fs::write(&path, b"descriptor")?;
    let file = File::open(&path)?;
    let mut output = blank_stat();

    assert_eq!(c::fstat(file.as_raw_fd(), &mut output), 0);
    assert_fields_equal(&output, &file.metadata()?);

    // A second query proves the wrapper did not close the descriptor.
    output = blank_stat();
    assert_eq!(c::fstat(file.as_raw_fd(), &mut output), 0);
    assert_eq!(output.st_size, 10);
    Ok(())
}

#[test]
fn fstat_supports_non_file_descriptors() -> io::Result<()> {
    let (stream, _peer) = UnixStream::pair()?;
    let mut output = blank_stat();
    assert_eq!(c::fstat(stream.as_raw_fd(), &mut output), 0);
    assert!(c::S_ISSOCK(output.st_mode));
    Ok(())
}

#[test]
fn fstat_rejects_invalid_descriptor_without_changing_output() {
    let mut output = blank_stat();
    output.st_dev = 11;
    output.st_ino = 22;
    output.st_mode = 33;
    output.st_size = 44;
    output.st_mtime_nsec = 55;
    let before = output;

    errno::set_errno(errno::Errno(0));
    assert_eq!(c::fstat(-1, &mut output), -1);
    assert_eq!(errno::errno(), errno::Errno(libc::EBADF));
    assert_stat_fields_equal(&output, &before);
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn stat64_variants_preserve_large_file_size() -> io::Result<()> {
    let dir = TestDir::new()?;
    let path = dir.join("large");
    let file = File::create(&path)?;
    let size = (1_u64 << 33) + 123;
    file.set_len(size)?;
    let metadata = file.metadata()?;
    let path_arg = zpath(&path);

    let mut by_path = blank_stat64();
    assert_eq!(c::stat64(&path_arg, &mut by_path), 0);
    assert_fields_equal64(&by_path, &metadata);
    assert_eq!(by_path.st_size, size as libc::off64_t);

    let mut by_file = blank_stat64();
    assert_eq!(c::fstat64_f(&file, &mut by_file), 0);
    assert_fields_equal64(&by_file, &metadata);

    let mut by_descriptor = blank_stat64();
    assert_eq!(c::fstat64(file.as_raw_fd(), &mut by_descriptor), 0);
    assert_fields_equal64(&by_descriptor, &metadata);

    let link = dir.join("link");
    symlink("large", &link)?;
    let mut link_output = blank_stat64();
    assert_eq!(c::lstat64(&zpath(&link), &mut link_output), 0);
    assert_fields_equal64(&link_output, &fs::symlink_metadata(link)?);
    assert!(c::S_ISLNK(link_output.st_mode));
    Ok(())
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn stat64_failures_set_errno_and_preserve_output() -> io::Result<()> {
    let dir = TestDir::new()?;
    let missing = zpath(&dir.join("missing"));
    let mut output = blank_stat64();
    output.st_dev = 11;
    output.st_ino = 22;
    output.st_mode = 33;
    output.st_size = 44;
    output.st_mtime_nsec = 55;
    let before = output;

    errno::set_errno(errno::Errno(0));
    assert_eq!(c::stat64(&missing, &mut output), -1);
    assert_eq!(errno::errno(), errno::Errno(libc::ENOENT));
    assert_stat64_fields_equal(&output, &before);

    errno::set_errno(errno::Errno(0));
    assert_eq!(c::lstat64(&missing, &mut output), -1);
    assert_eq!(errno::errno(), errno::Errno(libc::ENOENT));
    assert_stat64_fields_equal(&output, &before);

    errno::set_errno(errno::Errno(0));
    assert_eq!(c::fstat64(-1, &mut output), -1);
    assert_eq!(errno::errno(), errno::Errno(libc::EBADF));
    assert_stat64_fields_equal(&output, &before);
    Ok(())
}

#[test]
fn failure_sets_errno_and_leaves_output_unchanged() -> io::Result<()> {
    let dir = TestDir::new()?;
    let missing = zpath(&dir.join("missing"));
    let mut output = blank_stat();
    output.st_dev = 11;
    output.st_ino = 22;
    output.st_mode = 33;
    output.st_size = 44;
    output.st_mtime_nsec = 55;
    let before = output;

    errno::set_errno(errno::Errno(0));
    assert_eq!(c::stat(&missing, &mut output), -1);
    assert_eq!(errno::errno(), errno::Errno(libc::ENOENT));
    assert_stat_fields_equal(&output, &before);

    errno::set_errno(errno::Errno(0));
    assert_eq!(c::lstat(&missing, &mut output), -1);
    assert_eq!(errno::errno(), errno::Errno(libc::ENOENT));
    assert_stat_fields_equal(&output, &before);
    Ok(())
}

#[test]
fn c_mode_items_are_available_in_the_c_module() {
    assert!(c::S_ISREG(c::S_IFREG | c::S_IRUSR));
    assert!(c::S_ISDIR(c::S_IFDIR | c::S_IXOTH));
}
