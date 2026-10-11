// The C constants can have different integer widths on different Unix targets.
#![allow(clippy::unnecessary_cast)]
#![allow(non_snake_case)]

macro_rules! mode_constants {
    ($($name:ident: $doc:literal),+ $(,)?) => {
        $(
            #[doc = $doc]
            pub const $name: u32 = libc::$name as u32;
        )+
    };
}

mode_constants! {
    S_IFMT: "Mask selecting the file type bits.",
    S_IFREG: "Regular file type.",
    S_IFDIR: "Directory file type.",
    S_IFLNK: "Symbolic link file type.",
    S_IFCHR: "Character device file type.",
    S_IFBLK: "Block device file type.",
    S_IFIFO: "FIFO (named pipe) file type.",
    S_IFSOCK: "Socket file type.",
    S_ISUID: "Set-user-ID bit.",
    S_ISGID: "Set-group-ID bit.",
    S_ISVTX: "Sticky bit.",
    S_IRUSR: "Owner read permission.",
    S_IWUSR: "Owner write permission.",
    S_IXUSR: "Owner execute permission.",
    S_IRWXU: "Mask selecting all owner permissions.",
    S_IRGRP: "Group read permission.",
    S_IWGRP: "Group write permission.",
    S_IXGRP: "Group execute permission.",
    S_IRWXG: "Mask selecting all group permissions.",
    S_IROTH: "Other read permission.",
    S_IWOTH: "Other write permission.",
    S_IXOTH: "Other execute permission.",
    S_IRWXO: "Mask selecting all other permissions.",
}

/// Historical alias for owner read permission.
pub const S_IREAD: u32 = S_IRUSR;
/// Historical alias for owner write permission.
pub const S_IWRITE: u32 = S_IWUSR;
/// Historical alias for owner execute permission.
pub const S_IEXEC: u32 = S_IXUSR;
/// Mask selecting all access permission bits.
pub const ACCESSPERMS: u32 = S_IRWXU | S_IRWXG | S_IRWXO;
/// Mask selecting all access permissions and special mode bits.
pub const ALLPERMS: u32 = ACCESSPERMS | S_ISUID | S_ISGID | S_ISVTX;
/// Default regular file creation mode, before application of the process umask.
pub const DEFFILEMODE: u32 = S_IRUSR | S_IWUSR | S_IRGRP | S_IWGRP | S_IROTH | S_IWOTH;
/// Size in bytes of a block counted by `Stat::st_blocks`.
pub const S_BLKSIZE: u64 = 512;

macro_rules! type_predicates {
    ($($name:ident, $kind:ident: $doc:literal),+ $(,)?) => {
        $(
            #[doc = $doc]
            pub const fn $name(mode: u32) -> bool {
                mode & S_IFMT == $kind
            }
        )+
    };
}

type_predicates! {
    S_ISREG, S_IFREG: "Tests whether a mode describes a regular file.",
    S_ISDIR, S_IFDIR: "Tests whether a mode describes a directory.",
    S_ISLNK, S_IFLNK: "Tests whether a mode describes a symbolic link.",
    S_ISCHR, S_IFCHR: "Tests whether a mode describes a character device.",
    S_ISBLK, S_IFBLK: "Tests whether a mode describes a block device.",
    S_ISFIFO, S_IFIFO: "Tests whether a mode describes a FIFO (named pipe).",
    S_ISSOCK, S_IFSOCK: "Tests whether a mode describes a socket.",
}
