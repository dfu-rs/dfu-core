use super::*;
use std::prelude::v1::*;

/// A basic sync implementation that uses a `DfuIo` provided in argument during runtime.
pub struct DfuSync<IO, E>
where
    IO: DfuIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    dfu: DfuSansIo<IO>,
    buffer: Vec<u8>,
    progress: Option<Box<dyn Fn(usize)>>,
}

impl<IO, E> DfuSync<IO, E>
where
    IO: DfuIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    // TODO move the address?
    /// Create a new instance to a basic synchronous DFU implementation that uses the IO object
    /// provided in argument.
    pub fn new(io: IO, address: u32) -> Self {
        let transfer_size = io.functional_descriptor().transfer_size as usize;

        Self {
            dfu: DfuSansIo::new(io, address),
            buffer: vec![0x00; transfer_size],
            progress: None,
        }
    }

    /// Report progress synchronously to a closure.
    pub fn with_progress(self, progress: impl Fn(usize) + 'static) -> Self {
        Self {
            progress: Some(Box::new(progress)),
            ..self
        }
    }
}

impl<IO, E> DfuSync<IO, E>
where
    IO: DfuIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    /// Synchronously write a firmware to the device.
    pub fn download<R: std::io::Read>(&mut self, reader: R, length: u32) -> Result<(), IO::Error> {
        use std::io::BufRead;

        let transfer_size = self.dfu.io.functional_descriptor().transfer_size as usize;
        let mut reader = std::io::BufReader::with_capacity(transfer_size, reader);
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }

        macro_rules! wait_status {
            ($cmd:expr) => {{
                let mut cmd = $cmd;
                loop {
                    cmd = match cmd.next() {
                        get_status::Step::Break(cmd) => break cmd,
                        get_status::Step::Wait(cmd, poll_timeout) => {
                            std::thread::sleep(std::time::Duration::from_millis(poll_timeout));
                            let (cmd, n) = cmd.get_status(&mut self.buffer)?;
                            cmd.chain(&self.buffer[..n])?
                        }
                        get_status::Step::ManifestWaitReset(None) => return Ok(()),
                        get_status::Step::ManifestWaitReset(Some(cmd)) => {
                            let (_, res) = cmd.reset();
                            res?;
                            return Ok(());
                        }
                    };
                }
            }};
        }

        let cmd = self.dfu.download(length)?;
        let (cmd, _) = cmd.clear()?;
        let (cmd, n) = cmd.get_status(&mut self.buffer)?;
        let mut download_loop = cmd.chain(&self.buffer[..n])??;

        loop {
            download_loop = match download_loop.next() {
                download::Step::Break => break,
                download::Step::Erase(cmd) => {
                    let (cmd, _) = cmd.erase()?;
                    wait_status!(cmd)
                }
                download::Step::SetAddress(cmd) => {
                    let (cmd, _) = cmd.set_address()?;
                    wait_status!(cmd)
                }
                download::Step::DownloadChunk(cmd) => {
                    let chunk = reader.fill_buf()?;
                    let (cmd, n) = cmd.write(chunk)?;
                    reader.consume(n);
                    if let Some(progress) = self.progress.as_ref() {
                        progress(n);
                    }
                    wait_status!(cmd)
                }
            }
        }

        Ok(())
    }
}
