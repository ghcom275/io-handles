use io_handles::{ReadHandle, WriteHandle};
use std::io::copy;

fn main() -> anyhow::Result<()> {
    let mut input = ReadHandle::str("hello world\n")?;
    let mut output = WriteHandle::stdout()?;
    copy(&mut input, &mut output)?;
    Ok(())
}
