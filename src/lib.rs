//! Unbuffered and unlocked I/O streams.
//!
//! For a starting point, see [`ReadHandle`] and [`WriteHandle`] for input and
//! output streams. There's also [`ReadWriteHandle`] for interactive streams.
//!
//! Since these types are unbuffered, it's advisable for most use cases to wrap
//! them in buffering types such as [`std::io::BufReader`], [`std::io::BufWriter`],
//! [`std::io::LineWriter`], [`BufReaderWriter`], or [`BufReaderLineWriter`].
//!
//! [`BufReader`]: std::io::BufReader
//! [`BufWriter`]: std::io::BufWriter
//! [`LineWriter`]: std::io::LineWriter
//! [`AsRawFd`]: std::os::unix::io::AsRawFd
//! [pipe]: https://crates.io/crates/os_pipe

#![deny(missing_docs)]
#![cfg_attr(can_vector, feature(can_vector))]
#![cfg_attr(write_all_vectored, feature(write_all_vectored))]
#![cfg_attr(read_initializer, feature(read_initializer))]
#![cfg_attr(target_os = "wasi", feature(wasi_ext))]

mod buffered;
#[cfg(windows)]
mod descriptor;
mod lockers;
#[cfg(not(windows))]
mod posish;
mod read_write;
#[cfg(windows)]
mod winx;

pub use buffered::{BufReaderLineWriter, BufReaderWriter, IntoInnerError};
#[cfg(not(windows))]
pub use posish::{ReadHandle, ReadWriteHandle, WriteHandle};
#[cfg(not(windows))]
pub use read_write::AsRawReadWriteFd;
pub use read_write::ReadWrite;
#[cfg(windows)]
pub use read_write::{AsRawHandleOrSocket, AsRawReadWriteHandleOrSocket};
#[cfg(windows)]
pub use winx::{ReadHandle, ReadWriteHandle, WriteHandle};
