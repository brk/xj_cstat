#![cfg(unix)]
#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::fs::{self, File, FileTimes, Permissions};
use std::io::{self, Read, Seek, Write};
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use xj_cstat::*;

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!(
                "xj_cstat-{}-{}",
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

#[test]
fn regular_file_permissions_and_hard_links() -> io::Result<()> {
    let dir = TestDir::new()?;
    let path = dir.join("file");
    fs::write(&path, b"hello")?;
    fs::set_permissions(&path, Permissions::from_mode(0o640))?;
    fs::hard_link(&path, dir.join("hard-link"))?;

    let info = stat(&zpath(&path))?;
    assert!(S_ISREG(info.st_mode));
    assert_eq!(info.st_mode & ACCESSPERMS, 0o640);
    assert_eq!(info.st_size, 5);
    assert_eq!(info.st_nlink, 2);
    let linked = stat(&zpath(&dir.join("hard-link")))?;
    assert_eq!((info.st_dev, info.st_ino), (linked.st_dev, linked.st_ino));

    let metadata = fs::metadata(&path)?;
    assert_eq!(info.st_uid, metadata.uid());
    assert_eq!(info.st_gid, metadata.gid());
    assert_eq!(info.st_rdev, metadata.rdev());
    assert_eq!(info.st_blksize, metadata.blksize());
    assert_eq!(info.st_blocks, metadata.blocks());
    assert_eq!(info.st_ctim().tv_sec, metadata.ctime());
    assert_eq!(info.st_ctim().tv_nsec, metadata.ctime_nsec());
    assert_eq!(info, Stat::from(&metadata));
    assert_eq!(info, Stat::from(metadata));
    assert_eq!(info, lstat(&zpath(&path))?);
    assert_eq!(info, fstat(&File::open(&path)?)?);
    Ok(())
}

#[test]
fn symlink_and_dangling_symlink_semantics() -> io::Result<()> {
    let dir = TestDir::new()?;
    let target = dir.join("target");
    let link = dir.join("link");
    fs::write(&target, b"contents")?;
    symlink("target", &link)?;

    assert_eq!(stat(&zpath(&target))?, stat(&zpath(&link))?);
    let link_info = lstat(&zpath(&link))?;
    assert!(S_ISLNK(link_info.st_mode));
    assert_eq!(link_info.st_size, 6);

    fs::remove_file(&target)?;
    assert_eq!(
        stat(&zpath(&link)).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert!(S_ISLNK(lstat(&zpath(&link))?.st_mode));
    Ok(())
}

#[test]
fn lstat_follows_intermediate_links() -> io::Result<()> {
    let dir = TestDir::new()?;
    fs::create_dir(dir.join("directory"))?;
    fs::write(dir.join("directory/file"), b"data")?;
    symlink("directory", dir.join("alias"))?;
    assert!(S_ISREG(lstat(&zpath(&dir.join("alias/file")))?.st_mode));
    assert!(S_ISDIR(lstat(&zpath(&dir.join("alias/")))?.st_mode));
    Ok(())
}

#[test]
fn fstat_survives_unlink_and_preserves_position() -> io::Result<()> {
    let dir = TestDir::new()?;
    let path = dir.join("file");
    fs::write(&path, b"abcdef")?;
    let mut file = File::open(&path)?;
    file.seek(io::SeekFrom::Start(2))?;
    fs::remove_file(&path)?;

    let info = fstat(&file)?;
    assert_eq!(info.st_nlink, 0);
    assert_eq!(info.st_size, 6);
    assert_eq!(file.stream_position()?, 2);
    let mut remaining = String::new();
    file.read_to_string(&mut remaining)?;
    assert_eq!(remaining, "cdef");
    Ok(())
}

#[test]
fn byte_paths_use_first_nul_and_track_mutation() -> io::Result<()> {
    let dir = TestDir::new()?;
    let path = dir.join(OsStr::from_bytes(b"file-\xff"));
    fs::write(&path, b"bytes")?;
    let mut bytes = path.as_os_str().as_bytes().to_vec();
    bytes.extend_from_slice(b"\0ignored/missing");
    let mut path_arg = ZString::new(bytes);
    assert!(path_arg.to_str().is_err());
    assert_eq!(stat(&path_arg)?.st_size, 5);
    assert_eq!(lstat(&path_arg)?.st_size, 5);

    path_arg[0] = 0;
    assert!(stat(&path_arg).is_err());
    assert!(lstat(&path_arg).is_err());
    path_arg.as_buffer_mut()[0] = b'/';
    assert_eq!(stat(&path_arg)?.st_size, 5);
    Ok(())
}

#[test]
fn errors_preserve_os_error_codes() -> io::Result<()> {
    let dir = TestDir::new()?;
    fs::write(dir.join("file"), b"data")?;
    for path in [dir.join("missing"), dir.join("file/child"), PathBuf::new()] {
        let expected = fs::metadata(&path).unwrap_err();
        let actual = stat(&zpath(&path)).unwrap_err();
        assert_eq!(actual.kind(), expected.kind());
        assert_eq!(actual.raw_os_error(), expected.raw_os_error());
        let expected = fs::symlink_metadata(&path).unwrap_err();
        let actual = lstat(&zpath(&path)).unwrap_err();
        assert_eq!(actual.kind(), expected.kind());
        assert_eq!(actual.raw_os_error(), expected.raw_os_error());
    }
    Ok(())
}

#[test]
fn directory_and_socket_types() -> io::Result<()> {
    let dir = TestDir::new()?;
    assert!(S_ISDIR(stat(&zpath(&dir.0))?.st_mode));
    assert!(S_ISDIR(fstat(&File::open(&dir.0)?)?.st_mode));
    let (socket, _peer) = UnixStream::pair()?;
    let socket_file = File::from(OwnedFd::from(socket));
    assert!(S_ISSOCK(fstat(&socket_file)?.st_mode));
    Ok(())
}

#[test]
fn sparse_file_size_exceeds_u32() -> io::Result<()> {
    let dir = TestDir::new()?;
    let path = dir.join("sparse");
    let mut file = File::create(&path)?;
    let size = (1_u64 << 32) + 123;
    file.set_len(size)?;
    file.write_all(b"x")?;
    let info = fstat(&file)?;
    assert_eq!(info.st_size, size);
    assert_eq!(info.st_blocks, file.metadata()?.blocks());
    Ok(())
}

#[test]
fn timestamps_preserve_nanoseconds_and_negative_seconds() -> io::Result<()> {
    let dir = TestDir::new()?;
    let file = File::create(dir.join("file"))?;
    let accessed = SystemTime::UNIX_EPOCH - Duration::new(1, 123_456_789);
    let modified = SystemTime::UNIX_EPOCH + Duration::new(1_234_567, 987_654_321);
    file.set_times(
        FileTimes::new()
            .set_accessed(accessed)
            .set_modified(modified),
    )?;
    let info = fstat(&file)?;
    let metadata = file.metadata()?;
    assert_eq!(
        info.st_atim(),
        Timespec {
            tv_sec: metadata.atime(),
            tv_nsec: metadata.atime_nsec()
        }
    );
    assert_eq!(
        info.st_mtim(),
        Timespec {
            tv_sec: metadata.mtime(),
            tv_nsec: metadata.mtime_nsec()
        }
    );
    // Filesystems can round timestamps; check exact precision on Linux's test FS.
    #[cfg(target_os = "linux")]
    {
        assert_eq!(
            info.st_atim(),
            Timespec {
                tv_sec: -2,
                tv_nsec: 876_543_211
            }
        );
        assert_eq!(
            info.st_mtim(),
            Timespec {
                tv_sec: 1_234_567,
                tv_nsec: 987_654_321
            }
        );
    }
    Ok(())
}

#[test]
fn type_predicates_mask_permissions_and_are_exclusive() {
    type Predicate = fn(u32) -> bool;
    let cases: [(u32, Predicate); 7] = [
        (S_IFREG, S_ISREG),
        (S_IFDIR, S_ISDIR),
        (S_IFLNK, S_ISLNK),
        (S_IFCHR, S_ISCHR),
        (S_IFBLK, S_ISBLK),
        (S_IFIFO, S_ISFIFO),
        (S_IFSOCK, S_ISSOCK),
    ];
    for (kind, predicate) in cases {
        for permissions in [0, 0o640, ALLPERMS] {
            assert!(predicate(kind | permissions));
            for (other_kind, other_predicate) in cases {
                assert_eq!(other_predicate(kind | permissions), other_kind == kind);
            }
        }
        assert!(!predicate(ALLPERMS));
    }
    assert_eq!(ACCESSPERMS, 0o777);
    assert_eq!(ALLPERMS, 0o7777);
    assert_eq!(DEFFILEMODE, 0o666);
    assert_eq!((S_IREAD, S_IWRITE, S_IEXEC), (S_IRUSR, S_IWUSR, S_IXUSR));
}
