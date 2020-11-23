<div align="center">
  <h1><code>io-handles</code></h1>

  <p>
    <strong>Unbuffered and unlocked I/O streams</strong>
  </p>

  <p>
    <a href="https://github.com/sunfishcode/io-handles/actions?query=workflow%3ACI"><img src="https://github.com/sunfishcode/io-handles/workflows/CI/badge.svg" alt="Github Actions CI Status" /></a>
    <a href="https://crates.io/crates/io_handles"><img src="https://img.shields.io/crates/v/io_handles.svg" alt="crates.io page" /></a>
    <a href="https://docs.rs/io-handles"><img src="https://docs.rs/io-handles/badge.svg" alt="docs.rs docs" /></a>
  </p>
</div>

This crate defines [`ReadHandle`] and [`WriteHandle`] types which provide
unbuffered and unlocked access to a raw I/O stream, such as standard input,
standard output, files, sockets, or pipes. It also supports a "piped thread"
concept, where an arbitrary `Box<dyn Read + Send>` or `Box<dyn Write + Send>`
can be provided, and the I/O is performed on a thread and connecting to the
`ReadHandle` or `WriteHandle` with a [pipe].

On Posix-ish platforms, including limited support for WASI, these types just
contain a single file descriptor (and implement [`AsRawFd`]), plus any
resources needed to safely hold the file descriptor live. On Windows, they
contain an enum holding either `RawHandle` or `RawSocket` in place of the file
descriptor.

It also defines [`ReadWriteHandle`], which combines [`ReadHandle`] and
[`WriteHandle`], holding one (eg. a TCP socket) or two (eg. a pair of pipes)
file descriptors, to form an interactive stream.

Since these types are unbuffered, it's advisable for most use cases to wrap
them in buffering types such as [`std::io::BufReader`], [`std::io::BufWriter`],
[`std::io::LineWriter`], [`io_handles::BufReaderWriter`], or
[`io_handles::BufReaderLineWriter`].

Rust's [`std::io::Stdin`] and [`std::io::Stdout`] are always buffered, while
its [`std::fs::File`] and [`std::net::TcpStream`] are unbuffered. A key purpose
of the `io_handles` crate is to abstract over the underlying inputs and outputs
without adding buffering, so that buffering can be applied without redundancy.

This crate locks `stdio::io::Stdin` and `std::io::Stdout` while it has their
corresponding streams open, to prevent accidental mixing of buffered and
unbuffered output on the same stream. Attempts to use the buffered streams when
they are locked will block indefinitely.

[`ReadHandle`]: https://docs.rs/io-handles/latest/io_handles/struct.ReadHandle.html
[`WriteHandle`]: https://docs.rs/io-handles/latest/io_handles/struct.WriteHandle.html
[`ReadWriteHandle`]: https://docs.rs/io-handles/latest/io_handles/struct.ReadWriteHandle.html
[`io_handles::BufReaderWriter`]: https://docs.rs/io-handles/latest/io_handles/struct.BufReaderWriter.html
[`io_handles::BufReaderLineWriter`]: https://docs.rs/io-handles/latest/io_handles/struct.BufReaderLineWriter.html
[`std::io::Stdin`]: https://doc.rust-lang.org/std/io/struct.Stdin.html
[`std::io::Stdout`]: https://doc.rust-lang.org/std/io/struct.Stdout.html
[`std::io::BufReader`]: https://doc.rust-lang.org/std/io/struct.BufReader.html
[`std::io::BufWriter`]: https://doc.rust-lang.org/std/io/struct.BufWriter.html
[`std::io::LineWriter`]: https://doc.rust-lang.org/std/io/struct.LineWriter.html
[`AsRawFd`]: https://doc.rust-lang.org/std/os/unix/io/trait.AsRawFd.html
[pipe]: https://crates.io/crates/os_pipe
[`std::fs::File`]: https://doc.rust-lang.org/std/fs/struct.File.html
[`std::net::TcpStream`]: https://doc.rust-lang.org/std/net/struct.TcpStream.html
