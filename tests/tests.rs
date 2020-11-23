use io_handles::{ReadHandle, WriteHandle};
use std::{
    fs::{remove_file, File},
    io::{copy, Read, Write},
};
use tempfile::{tempdir, TempDir};

#[allow(unused)]
fn tmpdir() -> TempDir {
    tempdir().expect("expected to be able to create a temporary directory")
}

#[test]
fn test_copy() -> anyhow::Result<()> {
    let dir = tmpdir();
    let in_txt = dir.path().join("in.txt");
    let out_txt = dir.path().join("out.txt");

    let mut in_file = File::create(&in_txt)?;
    write!(in_file, "Hello, world!")?;

    // Test regular file I/O.
    {
        let mut input = ReadHandle::file(File::open(&in_txt)?);
        let mut output = WriteHandle::file(File::create(&out_txt)?);
        copy(&mut input, &mut output)?;
        output.flush()?;
        let mut s = String::new();
        File::open(&out_txt)?.read_to_string(&mut s)?;
        assert_eq!(s, "Hello, world!");
        remove_file(&out_txt)?;
    }

    // Test I/O through piped threads.
    {
        let mut input = ReadHandle::piped_thread(Box::new(File::open(&in_txt)?))?;
        let mut output = WriteHandle::piped_thread(Box::new(File::create(&out_txt)?))?;
        copy(&mut input, &mut output)?;
        output.flush()?;
        let mut s = String::new();
        File::open(&out_txt)?.read_to_string(&mut s)?;
        assert_eq!(s, "Hello, world!");
        remove_file(&out_txt)?;
    }

    // Test regular file I/O through piped threads, not because this is
    // amazingly useful, but because these things should compose and we can.
    // This also tests that `ReadHandle` and `WriteHandle`
    // implement `Send`.
    {
        let mut input = ReadHandle::piped_thread(Box::new(ReadHandle::file(File::open(&in_txt)?)))?;
        let mut output =
            WriteHandle::piped_thread(Box::new(WriteHandle::file(File::create(&out_txt)?)))?;
        copy(&mut input, &mut output)?;
        output.flush()?;
        let mut s = String::new();
        File::open(&out_txt)?.read_to_string(&mut s)?;
        assert_eq!(s, "Hello, world!");
        remove_file(&out_txt)?;
    }

    // They compose with themselves too.
    {
        let mut input = ReadHandle::piped_thread(Box::new(ReadHandle::piped_thread(Box::new(
            ReadHandle::file(File::open(&in_txt)?),
        ))?))?;
        let mut output = WriteHandle::piped_thread(Box::new(WriteHandle::piped_thread(
            Box::new(WriteHandle::file(File::create(&out_txt)?)),
        )?))?;
        copy(&mut input, &mut output)?;
        output.flush()?;
        let mut s = String::new();
        File::open(&out_txt)?.read_to_string(&mut s)?;
        assert_eq!(s, "Hello, world!");
        remove_file(&out_txt)?;
    }

    // Test flushing between writes.
    {
        let mut input = ReadHandle::piped_thread(Box::new(ReadHandle::piped_thread(Box::new(
            ReadHandle::file(File::open(&in_txt)?),
        ))?))?;
        let mut output = WriteHandle::piped_thread(Box::new(WriteHandle::piped_thread(
            Box::new(WriteHandle::file(File::create(&out_txt)?)),
        )?))?;
        copy(&mut input, &mut output)?;
        output.flush()?;
        let mut s = String::new();
        File::open(&out_txt)?.read_to_string(&mut s)?;
        assert_eq!(s, "Hello, world!");
        input = ReadHandle::piped_thread(Box::new(ReadHandle::piped_thread(Box::new(
            ReadHandle::file(File::open(&in_txt)?),
        ))?))?;
        copy(&mut input, &mut output)?;
        output.flush()?;
        s = String::new();
        File::open(&out_txt)?.read_to_string(&mut s)?;
        assert_eq!(s, "Hello, world!Hello, world!");
        input = ReadHandle::piped_thread(Box::new(ReadHandle::piped_thread(Box::new(
            ReadHandle::file(File::open(&in_txt)?),
        ))?))?;
        copy(&mut input, &mut output)?;
        output.flush()?;
        s = String::new();
        File::open(&out_txt)?.read_to_string(&mut s)?;
        assert_eq!(s, "Hello, world!Hello, world!Hello, world!");
        remove_file(&out_txt)?;
    }

    Ok(())
}

#[test]
fn test_null() -> anyhow::Result<()> {
    let mut input = ReadHandle::str("send to null")?;
    let mut output = WriteHandle::null()?;
    copy(&mut input, &mut output)?;
    output.flush()?;
    Ok(())
}
