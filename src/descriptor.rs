//! Windows isn't quite everything-is-a-file-descriptor, so use an enum of
//! low-level handle-like types on Windows.
//!
//! It's reasonable to wonder whether this is trying too hard to make Windows
//! work like Unix. But in this case, the abstraction is quite thin, a simple
//! enum with only two cases so the overhead should be low. And, it's very
//! useful, because it allows reading and writing from any I/O source that can
//! logically be read from or written to. So it seems justified.

use std::{
    fmt::Arguments,
    fs::File,
    io::{self, IoSlice, IoSliceMut, Read, Write},
    mem::ManuallyDrop,
    net::TcpStream,
    os::windows::io::{FromRawHandle, FromRawSocket, RawHandle, RawSocket},
};

/// The `Descriptor` enum holding either a raw handle or a raw socket, allowing
/// it to behave similarly to a Unix file descriptor.
pub(crate) enum Descriptor {
    File(ManuallyDrop<File>),
    Socket(ManuallyDrop<TcpStream>),
}

impl Descriptor {
    /// # Safety
    ///
    /// The caller must ensure that the resources held by `raw_handle` outlives
    /// the resulting `Descriptor` instance.
    #[inline]
    pub(crate) unsafe fn raw_handle(raw_handle: RawHandle) -> Self {
        Self::File(ManuallyDrop::new(File::from_raw_handle(raw_handle)))
    }

    /// # Safety
    ///
    /// The caller must ensure that the resources held by `raw_handle` outlives
    /// the resulting `Descriptor` instance.
    #[inline]
    pub(crate) unsafe fn raw_socket(raw_socket: RawSocket) -> Self {
        Self::Socket(ManuallyDrop::new(TcpStream::from_raw_socket(raw_socket)))
    }
}

impl Read for Descriptor {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.read(buf),
            Self::Socket(socket) => socket.read(buf),
        }
    }

    #[inline]
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.read_vectored(bufs),
            Self::Socket(socket) => socket.read_vectored(bufs),
        }
    }

    #[cfg(can_vector)]
    #[inline]
    fn is_read_vectored(&self) -> bool {
        match self {
            Self::File(file) => file.is_read_vectored(),
            Self::Socket(socket) => socket.is_read_vectored(),
        }
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        match self {
            Self::File(file) => file.read_to_end(buf),
            Self::Socket(socket) => socket.read_to_end(buf),
        }
    }

    #[inline]
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        match self {
            Self::File(file) => file.read_to_string(buf),
            Self::Socket(socket) => socket.read_to_string(buf),
        }
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        match self {
            Self::File(file) => file.read_exact(buf),
            Self::Socket(socket) => socket.read_exact(buf),
        }
    }
}

impl Write for Descriptor {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.write(buf),
            Self::Socket(socket) => socket.write(buf),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::File(file) => file.flush(),
            Self::Socket(socket) => socket.flush(),
        }
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[IoSlice]) -> io::Result<usize> {
        match self {
            Self::File(file) => file.write_vectored(bufs),
            Self::Socket(socket) => socket.write_vectored(bufs),
        }
    }

    #[cfg(can_vector)]
    #[inline]
    fn is_write_vectored(&self) -> bool {
        match self {
            Self::File(file) => file.is_write_vectored(),
            Self::Socket(socket) => socket.is_write_vectored(),
        }
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self {
            Self::File(file) => file.write_all(buf),
            Self::Socket(socket) => socket.write_all(buf),
        }
    }

    #[cfg(write_all_vectored)]
    #[inline]
    fn write_all_vectored(&mut self, bufs: &mut [IoSlice]) -> io::Result<()> {
        match self {
            Self::File(file) => file.write_all_vectored(bufs),
            Self::Socket(socket) => socket.write_all_vectored(bufs),
        }
    }

    #[inline]
    fn write_fmt(&mut self, fmt: Arguments) -> io::Result<()> {
        match self {
            Self::File(file) => file.write_fmt(fmt),
            Self::Socket(socket) => socket.write_fmt(fmt),
        }
    }
}
