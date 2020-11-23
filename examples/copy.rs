use io_handles::{ReadHandle, WriteHandle};
use std::io::copy;

fn main() -> anyhow::Result<()> {
    let mut input = ReadHandle::stdin()?;
    let mut output = WriteHandle::stdout()?;
    copy(&mut input, &mut output)?;
    Ok(())
}
