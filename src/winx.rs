//! Windows read/write descriptor implementation.
//!
//! Windows does not have an everything-is-a-file-descriptor abstraction, so we
//! use an enum to abstract over different kinds of I/O objects.
//!
//! There's no `AsRawHandle` implementation, as not all kinds of I/O use handles.
//! There's an `as_raw_handle` function, but it returns an `Option` so that it
//! can fail. Similarly there's an `as_raw_socket` which returns an `Option`.

use crate::{
    descriptor::Descriptor,
    lockers::{StdinLocker, StdoutLocker},
    AsRawHandleOrSocket, AsRawReadWriteHandleOrSocket,
};
use os_pipe::{pipe, PipeReader, PipeWriter};
use std::{
    fmt::{self, Arguments, Debug},
    fs::File,
    io::{self, copy, Cursor, IoSlice, IoSliceMut, Read, Write},
    net::TcpStream,
    os::windows::io::{AsRawHandle, AsRawSocket, RawHandle, RawSocket},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    thread::{self, JoinHandle},
};

/// An unbuffered and unlocked input byte stream, abstracted over the source of
/// the input.
///
/// Since it is unbuffered, and since many input sources have high per-call
/// overhead, it is often beneficial to wrap this in a [`BufReader`].
///
/// [`BufReader`]: https://doc.rust-lang.org/std/io/struct.BufReader.html
pub struct ReadHandle {
    descriptor: Descriptor,
    resources: ReadResources,
}

/// An unbuffered and unlocked output byte stream, abstracted over the
/// destination of the output.
///
/// Since it is unbuffered, and since many destinations have high per-call
/// overhead, it is often beneficial to wrap this in a [`BufWriter`] or
/// [`LineWriter`].
///
/// [`BufWriter`]: https://doc.rust-lang.org/std/io/struct.BufWriter.html
/// [`LineWriter`]: https://doc.rust-lang.org/std/io/struct.LineWriter.html
pub struct WriteHandle {
    descriptor: Descriptor,
    resources: WriteResources,
}

/// An unbuffered and unlocked interactive combination input and output stream.
///
/// This may hold two file descriptors, one for reading and one for writing,
/// such as stdin and stdout, or it may hold one file descriptor for both
/// reading and writing, such as for a TCP socket.
///
/// There is no `file` constructor, even though [`File`] implements both `Read`
/// and `Write`, because normal files are not interactive. However, there is a
/// `char_device` constructor for character device files.
///
/// [`File`]: std::fs::File
pub struct ReadWriteHandle {
    read_descriptor: Descriptor,
    write_descriptor: Descriptor,
    resources: ReadWriteResources,
}

/// Additional resources that need to be held in order to keep the stream live.
enum ReadResources {
    File(File),
    TcpStream(TcpStream),
    PipeReader(PipeReader),
    Stdin(StdinLocker),
    PipedThread(Option<(PipeReader, JoinHandle<io::Result<()>>)>),
    Child(Child),
    ChildStdout(ChildStdout),
    ChildStderr(ChildStderr),
}

/// Additional resources that need to be held in order to keep the stream live.
enum WriteResources {
    File(File),
    TcpStream(TcpStream),
    PipeWriter(PipeWriter),
    Stdout(StdoutLocker),
    PipedThread(Option<(PipeWriter, JoinHandle<io::Result<Box<dyn Write + Send>>>)>),
    Child(Child),
    ChildStdin(ChildStdin),
}

/// Additional resources that need to be held in order to keep the stream live.
enum ReadWriteResources {
    PipeReaderWriter((PipeReader, PipeWriter)),
    StdinStdout((StdinLocker, StdoutLocker)),
    Child(Child),
    ChildStdoutStdin((ChildStdout, ChildStdin)),
    CharDevice(File),
    TcpStream(TcpStream),
}

impl ReadHandle {
    /// Read from standard input.
    ///
    /// Unlike [`std::io::stdin`], this `stdin` returns a stream which is
    /// unbuffered and unlocked.
    ///
    /// This acquires a [`std::io::StdinLock`] to prevent accesses to
    /// `std::io::Stdin` while this is live, and fails if a `ReadHandle` or
    /// `ReadWriteHandle` for standard input already exists.
    ///
    /// [`std::io::stdin`]: https://doc.rust-lang.org/std/io/fn.stdin.html`
    /// [`std::io::StdinLock`]: https://doc.rust-lang.org/std/io/struct.StdinLock.html
    #[inline]
    pub fn stdin() -> io::Result<Self> {
        let stdin_locker = StdinLocker::new()?;
        Ok(Self {
            descriptor: unsafe { Descriptor::raw_handle(stdin_locker.as_raw_handle()) },
            resources: ReadResources::Stdin(stdin_locker),
        })
    }

    /// Read from an open file, taking ownership of it.
    #[inline]
    pub fn file(file: File) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_handle(file.as_raw_handle()) },
            resources: ReadResources::File(file),
        }
    }

    /// Read from an open TCP stream, taking ownership of it.
    #[inline]
    pub fn tcp_stream(tcp_stream: TcpStream) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_socket(tcp_stream.as_raw_socket()) },
            resources: ReadResources::TcpStream(tcp_stream),
        }
    }

    /// Read from the reading end of an open pipe, taking ownership of it.
    #[inline]
    pub fn pipe_reader(pipe_reader: PipeReader) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_handle(pipe_reader.as_raw_handle()) },
            resources: ReadResources::PipeReader(pipe_reader),
        }
    }

    /// Spawn the given command and read from its standard output.
    pub fn read_from_command(mut command: Command) -> io::Result<Self> {
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        let mut child = command.spawn()?;
        let child_stdout = child.stdout.take().unwrap();
        let raw_handle = child_stdout.as_raw_handle();
        Ok(Self {
            descriptor: unsafe { Descriptor::raw_handle(raw_handle) },
            resources: ReadResources::Child(child),
        })
    }

    /// Read from a child process' standard output, taking ownership of it.
    #[inline]
    pub fn child_stdout(child_stdout: ChildStdout) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_handle(child_stdout.as_raw_handle()) },
            resources: ReadResources::ChildStdout(child_stdout),
        }
    }

    /// Read from a child process' standard error, taking ownership of it.
    #[inline]
    pub fn child_stderr(child_stderr: ChildStderr) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_handle(child_stderr.as_raw_handle()) },
            resources: ReadResources::ChildStderr(child_stderr),
        }
    }

    /// Read from a boxed `Read` implementation, taking ownership of it. This
    /// works by creating a new thread to read the data and write it through a
    /// pipe.
    pub fn piped_thread(mut boxed_read: Box<dyn Read + Send>) -> io::Result<Self> {
        let (pipe_reader, mut pipe_writer) = pipe()?;
        let join_handle = thread::Builder::new()
            .name("piped thread for boxed reader".to_owned())
            .spawn(move || copy(&mut *boxed_read, &mut pipe_writer).map(|_size| ()))?;
        Ok(Self {
            descriptor: unsafe { Descriptor::raw_handle(pipe_reader.as_raw_handle()) },
            resources: ReadResources::PipedThread(Some((pipe_reader, join_handle))),
        })
    }

    /// Read from the given string.
    #[inline]
    pub fn str<S: AsRef<str>>(s: S) -> io::Result<Self> {
        Self::bytes(s.as_ref().as_bytes())
    }

    /// Read from the given bytes.
    pub fn bytes(bytes: &[u8]) -> io::Result<Self> {
        // For now, just launch a thread.
        Self::piped_thread(Box::new(Cursor::new(bytes.to_vec())))
    }

    fn map_err(&mut self, e: io::Error) -> io::Error {
        match &mut self.resources {
            ReadResources::PipedThread(piped_thread) => {
                let (pipe_reader, join_handle) = piped_thread.take().unwrap();
                drop(pipe_reader);
                join_handle.join().unwrap().unwrap_err()
            }
            _ => e,
        }
    }
}

impl WriteHandle {
    /// Write to standard output.
    ///
    /// Unlike [`std::io::stdout`], this `stdout` returns a stream which is
    /// unbuffered and unlocked.
    ///
    /// This acquires a [`std::io::StdoutLock`] (in a non-recursive way) to
    /// prevent accesses to `std::io::Stdout` while this is live, and fails if
    /// a `WriteHandle` or `ReadWriteHandle` for standard output already
    /// exists.
    ///
    /// [`std::io::stdout`]: https://doc.rust-lang.org/std/io/fn.stdout.html`
    /// [`std::io::StdoutLock`]: https://doc.rust-lang.org/std/io/struct.StdoutLock.html
    #[inline]
    pub fn stdout() -> io::Result<Self> {
        let stdout_locker = StdoutLocker::new()?;
        Ok(Self {
            descriptor: unsafe { Descriptor::raw_handle(stdout_locker.as_raw_handle()) },
            resources: WriteResources::Stdout(stdout_locker),
        })
    }

    /// Write to an open file, taking ownership of it.
    #[inline]
    pub fn file(file: File) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_handle(file.as_raw_handle()) },
            resources: WriteResources::File(file),
        }
    }

    /// Write to an open TCP stream, taking ownership of it.
    #[inline]
    pub fn tcp_stream(tcp_stream: TcpStream) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_socket(tcp_stream.as_raw_socket()) },
            resources: WriteResources::TcpStream(tcp_stream),
        }
    }

    /// Write to the writing end of an open pipe, taking ownership of it.
    #[inline]
    pub fn pipe_writer(pipe_writer: PipeWriter) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_handle(pipe_writer.as_raw_handle()) },
            resources: WriteResources::PipeWriter(pipe_writer),
        }
    }

    /// Write to a boxed `Write` implementation, taking ownership of it. This
    /// works by creating a new thread to read the data through a pipe and
    /// write it.
    ///
    /// Writes to the pipe aren't synchronous with writes to the boxed `Write`
    /// implementation. To ensure data is flushed all the way through
    /// the thread and into the boxed `Write` implementation, call `flush()`,
    /// which synchronizes with the thread to ensure that is has completed
    /// writing all pending output.
    pub fn piped_thread(mut boxed_write: Box<dyn Write + Send>) -> io::Result<Self> {
        let (mut pipe_reader, pipe_writer) = pipe()?;
        let join_handle = thread::Builder::new()
            .name("piped thread for boxed writer".to_owned())
            .spawn(move || {
                copy(&mut pipe_reader, &mut *boxed_write)?;
                boxed_write.flush()?;
                Ok(boxed_write)
            })?;
        Ok(Self {
            descriptor: unsafe { Descriptor::raw_handle(pipe_writer.as_raw_handle()) },
            resources: WriteResources::PipedThread(Some((pipe_writer, join_handle))),
        })
    }

    /// Spawn the given command and write to its standard input. Its standard
    /// output is redirected to `Stdio::null()`.
    pub fn write_to_command(mut command: Command) -> io::Result<Self> {
        command.stdin(Stdio::piped());
        command.stdout(Stdio::null());
        let mut child = command.spawn()?;
        let child_stdin = child.stdin.take().unwrap();
        let raw_handle = child_stdin.as_raw_handle();
        Ok(Self {
            descriptor: unsafe { Descriptor::raw_handle(raw_handle) },
            resources: WriteResources::Child(child),
        })
    }

    /// Write to the given child standard input, taking ownership of it.
    #[inline]
    pub fn child_stdin(child_stdin: ChildStdin) -> Self {
        Self {
            descriptor: unsafe { Descriptor::raw_handle(child_stdin.as_raw_handle()) },
            resources: WriteResources::ChildStdin(child_stdin),
        }
    }

    /// Write to the null device, which ignores all data.
    pub fn null() -> io::Result<Self> {
        Ok(Self::file(File::create("NUL")?))
    }

    fn map_err(&mut self, e: io::Error) -> io::Error {
        match &mut self.resources {
            WriteResources::PipedThread(piped_thread) => {
                let (pipe_writer, join_handle) = piped_thread.take().unwrap();
                drop(pipe_writer);
                join_handle.join().unwrap().map(|_| ()).unwrap_err()
            }
            _ => e,
        }
    }
}

impl ReadWriteHandle {
    /// Interact with stdin and stdout, taking ownership of them.
    ///
    /// Unlike [`std::io::stdin`] and [`std::io::stdout`], this `stdin_stdout`
    /// returns a stream which is unbuffered and unlocked.
    ///
    /// This acquires a [`std::io::StdinLock`] and a [`std::io::StdoutLock`] to
    /// prevent accesses to `std::io::Stdin` and `std::io::Stdout` while this
    /// is live, and fails if a `ReadHandle` for standard input, a
    /// `WriteHandle` for standard output, or a `ReadWriteHandle` for standard
    /// input and standard output already exist.
    ///
    /// [`std::io::stdin`]: https://doc.rust-lang.org/std/io/fn.stdin.html`
    /// [`std::io::stdout`]: https://doc.rust-lang.org/std/io/fn.stdout.html`
    /// [`std::io::StdinLock`]: https://doc.rust-lang.org/std/io/struct.StdinLock.html
    /// [`std::io::StdoutLock`]: https://doc.rust-lang.org/std/io/struct.StdoutLock.html
    #[inline]
    pub fn stdin_stdout() -> io::Result<Self> {
        let stdin_locker = StdinLocker::new()?;
        let stdout_locker = StdoutLocker::new()?;
        Ok(Self {
            read_descriptor: unsafe { Descriptor::raw_handle(stdin_locker.as_raw_handle()) },
            write_descriptor: unsafe { Descriptor::raw_handle(stdout_locker.as_raw_handle()) },
            resources: ReadWriteResources::StdinStdout((stdin_locker, stdout_locker)),
        })
    }

    /// Spawn the given command and interact with its standard input and
    /// output.
    pub fn interact_with_command(mut command: Command) -> io::Result<Self> {
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        let mut child = command.spawn()?;
        let child_stdin = child.stdin.take().unwrap();
        let child_stdout = child.stdout.take().unwrap();
        let raw_read_handle = child_stdout.as_raw_handle();
        let raw_write_handle = child_stdin.as_raw_handle();
        Ok(Self {
            read_descriptor: unsafe { Descriptor::raw_handle(raw_read_handle) },
            write_descriptor: unsafe { Descriptor::raw_handle(raw_write_handle) },
            resources: ReadWriteResources::Child(child),
        })
    }

    /// Interact with a child process' stdout and stdin, taking ownership of
    /// them.
    #[inline]
    pub fn child_stdout_stdin(child_stdout: ChildStdout, child_stdin: ChildStdin) -> Self {
        Self {
            read_descriptor: unsafe { Descriptor::raw_handle(child_stdout.as_raw_handle()) },
            write_descriptor: unsafe { Descriptor::raw_handle(child_stdout.as_raw_handle()) },
            resources: ReadWriteResources::ChildStdoutStdin((child_stdout, child_stdin)),
        }
    }

    /// Interact with an open character device, taking ownership of it.
    #[inline]
    pub fn char_device(char_device: File) -> Self {
        let raw_handle = char_device.as_raw_handle();
        Self {
            read_descriptor: unsafe { Descriptor::raw_handle(raw_handle) },
            write_descriptor: unsafe { Descriptor::raw_handle(raw_handle) },
            resources: ReadWriteResources::CharDevice(char_device),
        }
    }

    /// Interact with an open TCP stream, taking ownership of it.
    #[inline]
    pub fn tcp_stream(tcp_stream: TcpStream) -> Self {
        let raw_socket = tcp_stream.as_raw_socket();
        Self {
            read_descriptor: unsafe { Descriptor::raw_socket(raw_socket) },
            write_descriptor: unsafe { Descriptor::raw_socket(raw_socket) },
            resources: ReadWriteResources::TcpStream(tcp_stream),
        }
    }

    /// Interact a pair of pipe streams, taking ownership of them.
    #[inline]
    pub fn pipe_reader_writer(pipe_reader: PipeReader, pipe_writer: PipeWriter) -> Self {
        Self {
            read_descriptor: unsafe { Descriptor::raw_handle(pipe_reader.as_raw_handle()) },
            write_descriptor: unsafe { Descriptor::raw_handle(pipe_writer.as_raw_handle()) },
            resources: ReadWriteResources::PipeReaderWriter((pipe_reader, pipe_writer)),
        }
    }

    fn map_err(&mut self, e: io::Error) -> io::Error {
        match &mut self.resources {
            _ => e,
        }
    }
}

impl Read for ReadHandle {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.descriptor.read(buf) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut]) -> io::Result<usize> {
        match self.descriptor.read_vectored(bufs) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[cfg(can_vector)]
    #[inline]
    fn is_read_vectored(&self) -> bool {
        self.descriptor.is_read_vectored()
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        match self.descriptor.read_to_end(buf) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        match self.descriptor.read_to_string(buf) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        match self.descriptor.read_exact(buf) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }
}

impl Write for WriteHandle {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.descriptor.write(buf) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self.descriptor.flush() {
            Ok(()) => {
                // There's no way to send a flush event through a pipe, so for
                // now, force a flush by closing the pipe, waiting for the
                // thread to exit, recover the boxed writer, and then wrap it
                // in a whole new piped thread.
                if let WriteResources::PipedThread(piped_thread) = &mut self.resources {
                    let (mut pipe_writer, join_handle) = piped_thread.take().unwrap();
                    pipe_writer.flush()?;
                    drop(pipe_writer);
                    let boxed_write = join_handle.join().unwrap().unwrap();
                    *self = Self::piped_thread(boxed_write)?;
                }
                Ok(())
            }
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[IoSlice]) -> io::Result<usize> {
        match self.descriptor.write_vectored(bufs) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[cfg(can_vector)]
    #[inline]
    fn is_write_vectored(&self) -> bool {
        self.descriptor.is_write_vectored()
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self.descriptor.write_all(buf) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[cfg(write_all_vectored)]
    #[inline]
    fn write_all_vectored(&mut self, bufs: &mut [IoSlice]) -> io::Result<()> {
        match self.descriptor.write_all_vectored(bufs) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn write_fmt(&mut self, fmt: Arguments) -> io::Result<()> {
        match self.descriptor.write_fmt(fmt) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }
}

impl Read for ReadWriteHandle {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.read_descriptor.read(buf) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn read_vectored(&mut self, bufs: &mut [IoSliceMut]) -> io::Result<usize> {
        match self.read_descriptor.read_vectored(bufs) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[cfg(can_vector)]
    #[inline]
    fn is_read_vectored(&self) -> bool {
        self.read_descriptor.is_read_vectored()
    }

    #[inline]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        match self.read_descriptor.read_to_end(buf) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        match self.read_descriptor.read_to_string(buf) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        match self.read_descriptor.read_exact(buf) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }
}

impl Write for ReadWriteHandle {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.write_descriptor.write(buf) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self.write_descriptor.flush() {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn write_vectored(&mut self, bufs: &[IoSlice]) -> io::Result<usize> {
        match self.write_descriptor.write_vectored(bufs) {
            Ok(size) => Ok(size),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[cfg(can_vector)]
    #[inline]
    fn is_write_vectored(&self) -> bool {
        self.write_descriptor.is_write_vectored()
    }

    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        match self.write_descriptor.write_all(buf) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[cfg(write_all_vectored)]
    #[inline]
    fn write_all_vectored(&mut self, bufs: &mut [IoSlice]) -> io::Result<()> {
        match self.write_descriptor.write_all_vectored(bufs) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }

    #[inline]
    fn write_fmt(&mut self, fmt: Arguments) -> io::Result<()> {
        match self.write_descriptor.write_fmt(fmt) {
            Ok(()) => Ok(()),
            Err(e) => Err(self.map_err(e)),
        }
    }
}

impl AsRawHandleOrSocket for ReadHandle {
    /// Like `AsRawHandle::as_raw_handle` but returns an `Option` because not
    /// all of our stream types have raw handles.
    #[inline]
    fn as_raw_handle(&self) -> Option<RawHandle> {
        match &self.descriptor {
            Descriptor::File(file) => Some(file.as_raw_handle()),
            Descriptor::Socket(_) => None,
        }
    }

    /// Like `AsRawSocket::as_raw_socket` but returns an `Option` because not
    /// all of our stream types have raw sockets.
    #[inline]
    fn as_raw_socket(&self) -> Option<RawSocket> {
        match &self.descriptor {
            Descriptor::File(_) => None,
            Descriptor::Socket(socket) => Some(socket.as_raw_socket()),
        }
    }
}

impl AsRawHandleOrSocket for WriteHandle {
    fn as_raw_handle(&self) -> Option<RawHandle> {
        match &self.descriptor {
            Descriptor::File(file) => Some(file.as_raw_handle()),
            Descriptor::Socket(_) => None,
        }
    }

    fn as_raw_socket(&self) -> Option<RawSocket> {
        match &self.descriptor {
            Descriptor::File(_) => None,
            Descriptor::Socket(socket) => Some(socket.as_raw_socket()),
        }
    }
}

impl AsRawReadWriteHandleOrSocket for ReadWriteHandle {
    fn as_raw_read_handle(&self) -> Option<RawHandle> {
        match &self.read_descriptor {
            Descriptor::File(file) => Some(file.as_raw_handle()),
            Descriptor::Socket(_) => None,
        }
    }

    fn as_raw_write_handle(&self) -> Option<RawHandle> {
        match &self.write_descriptor {
            Descriptor::File(file) => Some(file.as_raw_handle()),
            Descriptor::Socket(_) => None,
        }
    }

    fn as_raw_read_socket(&self) -> Option<RawSocket> {
        match &self.read_descriptor {
            Descriptor::File(_) => None,
            Descriptor::Socket(socket) => Some(socket.as_raw_socket()),
        }
    }

    fn as_raw_write_socket(&self) -> Option<RawSocket> {
        match &self.write_descriptor {
            Descriptor::File(_) => None,
            Descriptor::Socket(socket) => Some(socket.as_raw_socket()),
        }
    }
}

impl Drop for ReadResources {
    fn drop(&mut self) {
        match self {
            Self::PipedThread(piped_thread) => {
                let (pipe_reader, join_handle) = piped_thread.take().unwrap();
                drop(pipe_reader);
                join_handle.join().unwrap().unwrap();
            }
            _ => {}
        }
    }
}

impl Drop for WriteResources {
    fn drop(&mut self) {
        match self {
            Self::PipedThread(piped_thread) => {
                if let Some((pipe_writer, join_handle)) = piped_thread.take() {
                    drop(pipe_writer);
                    join_handle.join().unwrap().unwrap();
                }
            }
            _ => {}
        }
    }
}

impl Drop for ReadWriteResources {
    fn drop(&mut self) {
        match self {
            _ => {}
        }
    }
}

impl Debug for ReadHandle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut b = f.debug_struct("ReadHandle");

        // Just print the raw handles; don't try to print the path or any
        // information about it, because this information is otherwise
        // unavailable to safe Rust code.
        b.field("raw_handle", &self.as_raw_handle());
        b.field("raw_socket", &self.as_raw_socket());

        // Don't print the resources, as we don't want to leak that
        // information through our abstraction.

        b.finish()
    }
}

impl Debug for WriteHandle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut b = f.debug_struct("WriteHandle");

        // Just print the raw handles; don't try to print the path or any
        // information about it, because this information is otherwise
        // unavailable to safe Rust code.
        b.field("raw_handle", &self.as_raw_handle());
        b.field("raw_socket", &self.as_raw_socket());

        // Don't print the resources, as we don't want to leak that
        // information through our abstraction.

        b.finish()
    }
}

impl Debug for ReadWriteHandle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut b = f.debug_struct("ReadWriteHandle");

        // Just print the raw handles; don't try to print the path or any
        // information about it, because this information is otherwise
        // unavailable to safe Rust code.
        b.field("raw_read_handle", &self.as_raw_read_handle());
        b.field("raw_read_socket", &self.as_raw_read_socket());
        b.field("raw_write_handle", &self.as_raw_write_handle());
        b.field("raw_write_socket", &self.as_raw_write_socket());

        // Don't print the resources, as we don't want to leak that
        // information through our abstraction.

        b.finish()
    }
}
