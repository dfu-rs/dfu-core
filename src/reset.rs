use super::*;

/// Command to reset the USB device.
#[must_use]
pub struct UsbReset<'dfu, IO: DfuIo, T> {
    pub(crate) dfu: &'dfu DfuSansIo<IO>,
    pub(crate) chained_command: T,
}

impl<'dfu, IO: DfuIo, T> UsbReset<'dfu, IO, T> {
    /// Reset the USB device.
    pub fn reset(self) -> (T, Result<IO::Reset, IO::Error>) {
        log::trace!("Device reset");
        let res = self.dfu.io.usb_reset();
        let next = self.chained_command;

        (next, res)
    }
}
