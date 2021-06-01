use super::*;

#[must_use]
pub struct ReadControl<'io, IO: DfuIo, T: ChainedCommandBytes> {
    pub(crate) io: &'io IO,
    pub(crate) request_type: u8,
    pub(crate) request: u8,
    pub(crate) value: u16,
    pub(crate) chained_command: T,
}

impl<'io, IO: DfuIo, T: ChainedCommandBytes> ReadControl<'io, IO, T> {
    pub fn send(self) -> Result<(ReadControlRecv<T>, IO::Read), IO::Error> {
        let res = self
            .io
            .read_control(self.request_type, self.request, self.value)?;
        let next = ReadControlRecv {
            chained_command: self.chained_command,
        };
        Ok((next, res))
    }
}

#[must_use]
pub struct ReadControlRecv<T: ChainedCommandBytes> {
    chained_command: T,
}

impl<T: ChainedCommandBytes> ReadControlRecv<T> {
    pub fn recv(self, bytes: &[u8]) -> T::Into {
        self.chained_command.chain(bytes)
    }
}
