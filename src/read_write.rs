use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::io::RawFd;
#[cfg(target_os = "wasi")]
use std::os::wasi::io::RawFd;
#[cfg(windows)]
use std::os::windows::io::{RawHandle, RawSocket};

/// A combination of [`std::io::Read`] and [`std::io::Write`] intended for use
/// in interactive I/O (as opposed to normal file I/O).
///
/// [`std::io::Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
/// [`std::io::Write`]: https://doc.rust-lang.org/std/io/trait.Write.html
pub trait ReadWrite: Read + Write {}

/// Like [`std::os::unix::io::AsRawFd`], but specifically for use with
/// [`ReadWrite`] implementations which may contain both reading and writing
/// file descriptors.
///
/// [`std::os::unix::io::AsRawFd`]: https://doc.rust-lang.org/std/os/unix/io/trait.AsRawFd.html
#[cfg(not(windows))]
pub trait AsRawReadWriteFd {
    /// Extracts the raw file descriptor for reading.
    ///
    /// Like [`std::os::unix::io::AsRawFd::as_raw_fd`], but returns the
    /// reading file descriptor of a [`ReadWrite`] implementation.
    ///
    /// [`std::os::unix::io::AsRawFd::as_raw_fd`]: https://doc.rust-lang.org/std/os/unix/io/trait.AsRawFd.html#tymethod.as_raw_fd
    fn as_raw_read_fd(&self) -> RawFd;

    /// Extracts the raw file descriptor for writing.
    ///
    /// Like [`std::os::unix::io::AsRawFd::as_raw_fd`], but returns the
    /// writing file descriptor of a [`ReadWrite`] implementation.
    ///
    /// [`std::os::unix::io::AsRawFd::as_raw_fd`]: https://doc.rust-lang.org/std/os/unix/io/trait.AsRawFd.html#tymethod.as_raw_fd
    fn as_raw_write_fd(&self) -> RawFd;
}

/// Like [`std::os::windows::io::AsRawHandle`] and
/// [`std::os::windows::io::AsRawSocket`], but for types which may or may not
/// contain a raw handle or raw socket at runtime.
#[cfg(windows)]
pub trait AsRawHandleOrSocket {
    /// Like [`std::os::windows::io::AsRawHandle::as_raw_handle`], but returns
    /// an `Option<RawHandle>` instead, for the case where there is no handle.
    fn as_raw_handle(&self) -> Option<RawHandle>;

    /// Like [`std::os::windows::io::AsRawSocket::as_raw_socket`], but returns
    /// an `Option<RawSocket>` instead, for the case where there is no socket.
    fn as_raw_socket(&self) -> Option<RawSocket>;
}

/// Like [`AsRawHandleOrSocket`], but specifically for use with [`ReadWrite`]
/// implementations which may contain both reading and writing file
/// descriptors.
#[cfg(windows)]
pub trait AsRawReadWriteHandleOrSocket {
    /// Like [`AsRawHandleOrSocket::as_raw_read_handle`], but returns
    /// an `Option<RawHandle>` instead, for the case where there is no handle.
    fn as_raw_read_handle(&self) -> Option<RawHandle>;

    /// Like [`AsRawHandleOrSocket::as_raw_write_handle`], but returns
    /// an `Option<RawHandle>` instead, for the case where there is no handle.
    fn as_raw_write_handle(&self) -> Option<RawHandle>;

    /// Like [`AsRawHandleOrSocket::as_raw_read_socket`], but returns
    /// an `Option<RawSocket>` instead, for the case where there is no socket.
    fn as_raw_read_socket(&self) -> Option<RawSocket>;

    /// Like [`AsRawHandleOrSocket::as_raw_write_socket`], but returns
    /// an `Option<RawSocket>` instead, for the case where there is no socket.
    fn as_raw_write_socket(&self) -> Option<RawSocket>;
}
