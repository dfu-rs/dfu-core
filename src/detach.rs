use super::*;

const REQUEST_TYPE: u8 = 0b00100001;
const DFU_DETACH: u8 = 0;

/// Command that sends `dfuDETACH` to the device.
#[must_use]
pub struct Detach<T> {
    pub(crate) descriptor: FunctionalDescriptor,
    pub(crate) chained_command: T,
}

impl<T> Detach<T> {
    /// Send the command `dfuDETACH` to the device.
    pub fn detach(self) -> (T, UsbWriteControl<[u8; 0]>) {
        log::trace!("Detaching device");
        let detach_timeout = self.descriptor.detach_timeout;
        let next = self.chained_command;
        let control = UsbWriteControl::new(REQUEST_TYPE, DFU_DETACH, detach_timeout, []);

        (next, control)
    }
}
