use super::*;

const REQUEST_TYPE: u8 = 0b00100001;
const DFU_GETSTATUS: u8 = 3;
const DFU_CLRSTATUS: u8 = 4;

pub(crate) type PollTimeout = u32;
pub(crate) type Index = u8;
pub(crate) type GetStatusMessage = (Status, PollTimeout, State, Index);

#[must_use]
pub struct GetStatus<'dfu, 'mem, IO: DfuIo, T: ChainedCommand<Arg = GetStatusMessage>> {
    pub(crate) dfu: &'dfu DfuSansIo<'mem, IO>,
    pub(crate) chained_command: T,
}

impl<'dfu, 'mem, IO: DfuIo, T: ChainedCommand<Arg = GetStatusMessage>>
    GetStatus<'dfu, 'mem, IO, T>
{
    pub fn query(self) -> control::ReadControl<'dfu, IO, GetStatusRecv<T>> {
        control::ReadControl {
            io: &self.dfu.io,
            request_type: 0b10100001,
            request: DFU_GETSTATUS,
            value: 0,
            chained_command: GetStatusRecv {
                chained_command: self.chained_command,
            },
        }
    }
}

#[must_use]
pub struct GetStatusRecv<T: ChainedCommand<Arg = GetStatusMessage>> {
    chained_command: T,
}

impl<T: ChainedCommand<Arg = GetStatusMessage>> ChainedCommandBytes for GetStatusRecv<T> {
    type Into = Result<T::Into, Error>;

    fn chain(self, bytes: &[u8]) -> Self::Into {
        let status: GetStatusMessage = todo!();
        Ok(self.chained_command.chain(status))
    }
}

#[must_use]
pub struct ClearStatus<'dfu, 'mem, IO: DfuIo, T> {
    pub(crate) dfu: &'dfu DfuSansIo<'mem, IO>,
    pub(crate) chained_command: T,
}

impl<'dfu, 'mem, IO: DfuIo, T> ClearStatus<'dfu, 'mem, IO, T> {
    pub fn clear(self) -> Result<(T, IO::Write), IO::Error> {
        let res = self
            .dfu
            .io
            .write_control(REQUEST_TYPE, DFU_CLRSTATUS, 0, &[])?;
        let next = self.chained_command;

        Ok((next, res))
    }
}
