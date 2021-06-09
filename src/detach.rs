use super::*;

const REQUEST_TYPE: u8 = 0b00100001;
const DFU_DETACH: u8 = 0;

#[must_use]
pub struct Detach<'dfu, IO: DfuIo, T> {
    pub(crate) dfu: &'dfu DfuSansIo<IO>,
    pub(crate) chained_command: T,
}

impl<'dfu, IO: DfuIo, T> Detach<'dfu, IO, T> {
    pub fn detach(self) -> Result<(T, IO::Write), IO::Error> {
        let detach_timeout = self.dfu.io.functional_descriptor().detach_timeout;
        let next = self.chained_command;
        let res = self
            .dfu
            .io
            .write_control(REQUEST_TYPE, DFU_DETACH, detach_timeout, &[])?;

        Ok((next, res))
    }
}
