use super::*;
use std::convert::TryFrom;
use std::io::Cursor;
use std::prelude::v1::*;

struct Buffer<R: std::io::Read> {
    reader: R,
    buf: Box<[u8]>,
    level: usize,
}

impl<R: std::io::Read> Buffer<R> {
    fn new(size: usize, reader: R) -> Self {
        Self {
            reader,
            buf: vec![0; size].into_boxed_slice(),
            level: 0,
        }
    }

    fn fill_buf(&mut self) -> Result<&[u8], std::io::Error> {
        while self.level < self.buf.len() {
            let dst = &mut self.buf[self.level..];
            let r = self.reader.read(dst)?;
            if r == 0 {
                break;
            } else {
                self.level += r;
            }
        }
        Ok(&self.buf[0..self.level])
    }

    fn consume(&mut self, amt: usize) {
        if amt >= self.level {
            self.level = 0;
        } else {
            self.buf.copy_within(amt..self.level, 0);
            self.level -= amt;
        }
    }
}

/// Generic synchronous implementation of DFU.
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub struct DfuSync<IO, E>
where
    IO: DfuIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    dfu: DfuSansIo<IO>,
    buffer: Vec<u8>,
    progress: Option<Box<dyn FnMut(usize)>>,
}

impl<IO, E> DfuSync<IO, E>
where
    IO: DfuIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    /// Create a new instance of a generic synchronous implementation of DFU.
    pub fn new(io: IO) -> Self {
        let transfer_size = io.functional_descriptor().transfer_size as usize;

        Self {
            dfu: DfuSansIo::new(io),
            buffer: vec![0x00; transfer_size],
            progress: None,
        }
    }

    /// Override the address onto which the firmware is downloaded.
    ///
    /// This address is only used if the device uses the DfuSe protocol.
    pub fn override_address(&mut self, address: u32) -> &mut Self {
        self.dfu.set_address(address);
        self
    }

    /// Use this closure to show progress.
    pub fn with_progress(&mut self, progress: impl FnMut(usize) + 'static) -> &mut Self {
        self.progress = Some(Box::new(progress));
        self
    }

    /// Consume the object and return its [`DfuIo`]
    pub fn into_inner(self) -> IO {
        self.dfu.into_inner()
    }
}

impl<IO, E> DfuSync<IO, E>
where
    IO: DfuIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    /// Download a firmware into the device from a slice.
    pub fn download_from_slice(&mut self, slice: &[u8]) -> Result<(), IO::Error> {
        let length = slice.len();
        let cursor = Cursor::new(slice);

        self.download(
            cursor,
            u32::try_from(length).map_err(|_| Error::OutOfCapabilities)?,
        )
    }

    /// Download a firmware into the device from a reader.
    pub fn download<R: std::io::Read>(&mut self, reader: R, length: u32) -> Result<(), IO::Error> {
        let transfer_size = self.dfu.io.functional_descriptor().transfer_size as usize;
        let mut reader = Buffer::new(transfer_size, reader);
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
                            cmd.chain(&self.buffer[..n])??
                        }
                    };
                }
            }};
        }

        let cmd = self.dfu.download(length)?;
        let (cmd, n) = cmd.get_status(&mut self.buffer)?;
        let (cmd, _) = cmd.chain(&self.buffer[..n])??;
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
                    let (cmd, n) = cmd.download(chunk)?;
                    reader.consume(n);
                    if let Some(progress) = self.progress.as_mut() {
                        progress(n);
                    }
                    wait_status!(cmd)
                }
                download::Step::UsbReset => {
                    log::trace!("Device reset");
                    self.dfu.io.usb_reset()?;
                    break;
                }
            }
        }

        Ok(())
    }

    /// Download a firmware into the device.
    ///
    /// The length is guest from the reader.
    pub fn download_all<R: std::io::Read + std::io::Seek>(
        &mut self,
        mut reader: R,
    ) -> Result<(), IO::Error> {
        let length = u32::try_from(reader.seek(std::io::SeekFrom::End(0))?)
            .map_err(|_| Error::MaximumTransferSizeExceeded)?;
        reader.seek(std::io::SeekFrom::Start(0))?;
        self.download(reader, length)
    }

    /// Send a Detach request to the device
    pub fn detach(&self) -> Result<(), IO::Error> {
        self.dfu.detach()
    }

    /// Reset the USB device
    pub fn usb_reset(&self) -> Result<IO::Reset, IO::Error> {
        self.dfu.usb_reset()
    }

    /// Returns whether the device is will detach if requested
    pub fn will_detach(&self) -> bool {
        self.dfu.will_detach()
    }

    /// Returns whether the device is manifestation tolerant
    pub fn manifestation_tolerant(&self) -> bool {
        self.dfu.manifestation_tolerant()
    }
}
