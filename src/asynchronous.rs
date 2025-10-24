use futures::{io::Cursor, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};

use super::*;
use core::future::Future;
use std::convert::TryFrom;
use std::prelude::v1::*;

/// Trait to implement lower level communication with a USB device.
pub trait DfuAsyncIo {
    /// Return type after calling [`Self::read_control`].
    type Read;
    /// Return type after calling [`Self::write_control`].
    type Write;
    /// Return type after calling [`Self::usb_reset`].
    type Reset;
    /// Error type.
    type Error: From<Error>;
    /// Dfuse Memory layout type
    type MemoryLayout: AsRef<memory_layout::mem>;

    /// Read data using control transfer.
    fn read_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &mut [u8],
    ) -> impl Future<Output = Result<Self::Read, Self::Error>> + Send;

    /// Write data using control transfer.
    fn write_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &[u8],
    ) -> impl Future<Output = Result<Self::Write, Self::Error>> + Send;

    /// Triggers a USB reset.
    fn usb_reset(&self) -> impl Future<Output = Result<Self::Reset, Self::Error>> + Send;

    /// Sleep for this duration of time.
    fn sleep(&self, duration: std::time::Duration) -> impl Future<Output = ()> + Send;

    /// Returns the protocol of the device
    fn protocol(&self) -> &DfuProtocol<Self::MemoryLayout>;

    /// Returns the functional descriptor of the device.
    fn functional_descriptor(&self) -> &functional_descriptor::FunctionalDescriptor;
}

impl UsbReadControl<'_> {
    /// Execute usb write using io
    pub async fn execute_async<IO: DfuAsyncIo>(&mut self, io: &IO) -> Result<IO::Read, IO::Error> {
        io.read_control(self.request_type, self.request, self.value, self.buffer)
            .await
    }
}

impl<D> UsbWriteControl<D>
where
    D: AsRef<[u8]>,
{
    /// Execute usb write using io
    pub async fn execute_async<IO: DfuAsyncIo>(&self, io: &IO) -> Result<IO::Write, IO::Error> {
        io.write_control(
            self.request_type,
            self.request,
            self.value,
            self.buffer.as_ref(),
        )
        .await
    }
}

struct Buffer<R: AsyncRead + Unpin> {
    reader: R,
    buf: Box<[u8]>,
    level: usize,
}

impl<R: AsyncRead + Unpin> Buffer<R> {
    fn new(size: usize, reader: R) -> Self {
        Self {
            reader,
            buf: vec![0; size].into_boxed_slice(),
            level: 0,
        }
    }

    async fn fill_buf(&mut self) -> Result<&[u8], std::io::Error> {
        while self.level < self.buf.len() {
            let dst = &mut self.buf[self.level..];
            let r = self.reader.read(dst).await?;
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

/// Generic asynchronous implementation of DFU.
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub struct DfuASync<IO, E>
where
    IO: DfuAsyncIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    io: IO,
    dfu: DfuSansIo,
    buffer: Vec<u8>,
}

impl<IO, E> DfuASync<IO, E>
where
    IO: DfuAsyncIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    /// Create a new instance of a generic synchronous implementation of DFU.
    pub fn new(io: IO) -> Self {
        let transfer_size = io.functional_descriptor().transfer_size as usize;
        let descriptor = *io.functional_descriptor();

        Self {
            io,
            dfu: DfuSansIo::new(descriptor),
            buffer: vec![0x00; transfer_size],
        }
    }

    /// Override the address onto which the firmware is downloaded.
    ///
    /// This address is only used if the device uses the DfuSe protocol.
    pub fn override_address(&mut self, address: u32) -> &mut Self {
        self.dfu.set_address(address);
        self
    }

    /// Consume the object and return its [`DfuIo`]
    pub fn into_inner(self) -> IO {
        self.io
    }
}

impl<IO, E> DfuASync<IO, E>
where
    IO: DfuAsyncIo<Read = usize, Write = usize, Reset = (), Error = E>,
    E: From<std::io::Error> + From<Error>,
{
    /// Download a firmware into the device from a slice.
    pub async fn download_from_slice(&mut self, slice: &[u8]) -> Result<(), IO::Error> {
        let length = slice.len();
        let cursor = Cursor::new(slice);

        self.download(
            cursor,
            u32::try_from(length).map_err(|_| Error::OutOfCapabilities)?,
        )
        .await
    }

    /// Download a firmware into the device from a reader.
    pub async fn download<R: AsyncReadExt + Unpin>(
        &mut self,
        reader: R,
        length: u32,
    ) -> Result<(), IO::Error> {
        let transfer_size = self.io.functional_descriptor().transfer_size as usize;
        let mut reader = Buffer::new(transfer_size, reader);
        let buffer = reader.fill_buf().await?;
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
                            self.io
                                .sleep(std::time::Duration::from_millis(poll_timeout))
                                .await;
                            let (cmd, mut control) = cmd.get_status(&mut self.buffer);
                            let n = control.execute_async(&self.io).await?;
                            cmd.chain(&self.buffer[..n as usize])??
                        }
                    };
                }
            }};
        }

        let cmd = self.dfu.download(self.io.protocol(), length)?;
        let (cmd, mut control) = cmd.get_status(&mut self.buffer);
        let n = control.execute_async(&self.io).await?;
        let (cmd, control) = cmd.chain(&self.buffer[..n])?;
        if let Some(control) = control {
            control.execute_async(&self.io).await?;
        }
        let (cmd, mut control) = cmd.get_status(&mut self.buffer);
        let n = control.execute_async(&self.io).await?;
        let mut download_loop = cmd.chain(&self.buffer[..n])??;

        loop {
            download_loop = match download_loop.next() {
                download::Step::Break => break,
                download::Step::Erase(cmd) => {
                    let (cmd, control) = cmd.erase()?;
                    control.execute_async(&self.io).await?;
                    wait_status!(cmd)
                }
                download::Step::SetAddress(cmd) => {
                    let (cmd, control) = cmd.set_address();
                    control.execute_async(&self.io).await?;
                    wait_status!(cmd)
                }
                download::Step::DownloadChunk(cmd) => {
                    let chunk = reader.fill_buf().await?;
                    let (cmd, control) = cmd.download(chunk)?;
                    let n = control.execute_async(&self.io).await?;
                    reader.consume(n);
                    wait_status!(cmd)
                }
                download::Step::UsbReset => {
                    log::trace!("Device reset");
                    self.io.usb_reset().await?;
                    break;
                }
            }
        }

        Ok(())
    }

    /// Download a firmware into the device.
    ///
    /// The length is guess from the reader.
    pub async fn download_all<R: AsyncReadExt + Unpin + AsyncSeek>(
        &mut self,
        mut reader: R,
    ) -> Result<(), IO::Error> {
        let length = u32::try_from(reader.seek(std::io::SeekFrom::End(0)).await?)
            .map_err(|_| Error::MaximumTransferSizeExceeded)?;
        reader.seek(std::io::SeekFrom::Start(0)).await?;
        self.download(reader, length).await
    }

    /// Send a Detach request to the device
    pub async fn detach(&self) -> Result<(), IO::Error> {
        self.dfu.detach().execute_async(&self.io).await?;
        Ok(())
    }

    /// Reset the USB device
    pub async fn usb_reset(&self) -> Result<IO::Reset, IO::Error> {
        self.io.usb_reset().await
    }

    /// Returns whether the device is will detach if requested
    pub fn will_detach(&self) -> bool {
        self.io.functional_descriptor().will_detach
    }

    /// Returns whether the device is manifestation tolerant
    pub fn manifestation_tolerant(&self) -> bool {
        self.io.functional_descriptor().manifestation_tolerant
    }
}
