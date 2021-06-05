use super::*;

pub struct DfuSync<IO, E>
where
    IO: DfuIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<crate::Error>,
{
    dfu: DfuSansIo<IO>,
    buffer: Vec<u8>,
    progress: Option<Box<dyn Fn(usize)>>,
}

impl<IO, E> DfuSync<IO, E>
where
    IO: DfuIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<crate::Error>,
{
    pub fn new(io: IO, address: u32, transfer_size: u32) -> Self {
        Self {
            dfu: DfuSansIo::new(io, address, transfer_size),
            buffer: vec![0x00; transfer_size as usize],
            progress: None,
        }
    }

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
    E: From<std::io::Error> + From<crate::Error>,
{
    pub fn download<R: std::io::Read>(&mut self, reader: R, length: u32) -> Result<(), IO::Error> {
        use crate::download;
        use crate::get_status;
        use std::io::BufRead;

        let mut reader = std::io::BufReader::with_capacity(self.dfu.transfer_size as usize, reader);
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }

        let cmd = self.dfu.download(length)?;
        let (cmd, _) = cmd.reset();
        let (cmd, _) = cmd.clear()?;
        let (cmd, n) = cmd.get_status(&mut self.buffer)?;
        let mut download_loop = cmd.chain(&self.buffer[..n])??;

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
                        get_status::Step::WaitManifest(mut cmd) => {
                            self.dfu.io.usb_reset()?;
                            return Ok(());
                            /*
                            loop {
                                // TODO arbitrary sleeping
                                std::thread::sleep(std::time::Duration::from_millis(100));
                                cmd = match cmd.get_status_manifest(&mut self.buffer) {
                                    get_status::WaitManifestStep::StatusReceived(cmd, n) => {
                                        break cmd.chain(&self.buffer[..n])?;
                                    }
                                    get_status::WaitManifestStep::StatusNotReceived(cmd) => cmd,
                                };
                            }
                            */
                        }
                    };
                }
            }};
        }

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
