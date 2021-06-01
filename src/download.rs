use super::*;

const REQUEST_TYPE: u8 = 0b00100001;
const DFU_DNLOAD: u8 = 3;

#[must_use]
pub struct Start<'dfu, 'mem, IO: DfuIo> {
    pub(crate) dfu: &'dfu DfuSansIo<'mem, IO>,
    pub(crate) memory_layout: &'mem memory_layout::mem,
    pub(crate) address: u32,
    pub(crate) length: Option<usize>,
}

impl<'dfu, 'mem, IO: DfuIo> ChainedCommand for Start<'dfu, 'mem, IO> {
    type Arg = get_status::GetStatusMessage;
    type Into = Result<Loop<'dfu, 'mem, IO>, Error>;

    fn chain(self, (_status, _poll_timeout, state, _index): Self::Arg) -> Self::Into {
        if state == State::DfuIdle {
            Ok(Loop {
                dfu: self.dfu,
                memory_layout: self.memory_layout,
                address: self.address,
                length: self.length,
                copied: 0,
                erased: 0,
                address_set: false,
                block_num: 2,
                eof: false,
            })
        } else {
            Err(Error::InvalidState {
                got: state,
                expected: State::DfuIdle,
            })
        }
    }
}

#[must_use]
pub struct Loop<'dfu, 'mem, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<'mem, IO>,
    memory_layout: &'mem memory_layout::mem,
    address: u32,
    length: Option<usize>,
    copied: u32,
    erased: u32,
    address_set: bool,
    block_num: u16,
    eof: bool,
}

impl<'dfu, 'mem, IO: DfuIo> Loop<'dfu, 'mem, IO> {
    pub fn next<'dl>(&'dl mut self) -> Option<Step<'dfu, 'mem, 'dl, IO>> {
        if self.eof {
            None
        } else if self.erased <= self.copied {
            Some(Step::Erase(ErasePage { dl: self }))
        } else if !self.address_set {
            Some(Step::SetAddress(SetAddress { dl: self }))
        } else {
            Some(Step::DownloadChunk(DownloadChunk { dl: self }))
        }
    }
}

pub enum Step<'dfu, 'mem, 'dl, IO: DfuIo> {
    Erase(ErasePage<'dfu, 'mem, 'dl, IO>),
    SetAddress(SetAddress<'dfu, 'mem, 'dl, IO>),
    DownloadChunk(DownloadChunk<'dfu, 'mem, 'dl, IO>),
}

#[must_use]
pub struct EnsureIdle;

impl ChainedCommand for EnsureIdle {
    type Arg = get_status::GetStatusMessage;
    type Into = Result<(), Error>;

    fn chain(self, (_status, _poll_timeout, state, _index): Self::Arg) -> Self::Into {
        if state == State::DfuDnloadIdle {
            Ok(())
        } else {
            Err(Error::InvalidState {
                got: state,
                expected: State::DfuDnloadIdle,
            })
        }
    }
}

impl<'dfu, 'mem, 'dl, IO: DfuIo> SetAddress<'dfu, 'mem, 'dl, IO> {
    pub fn set_address(
        self,
    ) -> Result<(get_status::GetStatus<'dfu, 'mem, IO, EnsureIdle>, IO::Write), IO::Error> {
        self.dl.address_set = true;

        let res = self.dl.dfu.io.write_control(
            REQUEST_TYPE,
            DFU_DNLOAD,
            0,
            &<[u8; 5]>::from(DownloadCommandSetAddress(self.dl.address)),
        )?;
        let next = get_status::GetStatus {
            dfu: &self.dl.dfu,
            chained_command: EnsureIdle,
        };

        Ok((next, res))
    }
}

#[must_use]
pub struct ErasePage<'dfu, 'mem, 'dl, IO: DfuIo> {
    dl: &'dl mut Loop<'dfu, 'mem, IO>,
}

impl<'dfu, 'mem, 'dl, IO: DfuIo> ErasePage<'dfu, 'mem, 'dl, IO> {
    pub fn erase(
        self,
    ) -> Result<(get_status::GetStatus<'dfu, 'mem, IO, EnsureIdle>, IO::Write), IO::Error> {
        let (page_size, rest) = self
            .dl
            .memory_layout
            .split_first()
            .ok_or_else(|| Error::NoSpaceLeft)?;
        self.dl.memory_layout = rest;

        self.dl.erased = self
            .dl
            .erased
            .checked_add(*page_size)
            .ok_or_else(|| Error::EraseLimitReached)?;
        self.dl.address_set = false;

        let res = self.dl.dfu.io.write_control(
            REQUEST_TYPE,
            DFU_DNLOAD,
            0,
            &<[u8; 5]>::from(DownloadCommandErase(self.dl.address)),
        )?;
        let next = get_status::GetStatus {
            dfu: &self.dl.dfu,
            chained_command: EnsureIdle,
        };

        Ok((next, res))
    }
}

#[must_use]
pub struct DownloadChunk<'dfu, 'mem, 'dl, IO: DfuIo> {
    dl: &'dl mut Loop<'dfu, 'mem, IO>,
}

impl<'dfu, 'mem, 'dl, IO: DfuIo> DownloadChunk<'dfu, 'mem, 'dl, IO> {
    pub fn write(
        self,
        bytes: &[u8],
    ) -> Result<
        (
            get_status::GetStatus<'dfu, 'mem, IO, EnsureIdle>,
            Option<IO::Write>,
        ),
        IO::Error,
    > {
        use std::convert::TryFrom;

        let next = get_status::GetStatus {
            dfu: &self.dl.dfu,
            chained_command: EnsureIdle,
        };

        if bytes.len() == 0 {
            return Ok((next, None));
        }

        let len = u32::try_from(bytes.len()).map_err(|_| Error::BufferTooBig {
            got: bytes.len(),
            expected: u32::MAX as usize,
        })?;
        let transfer_size = self.dl.dfu.transfer_size;
        let slice = if len >= transfer_size {
            self.dl.copied = self
                .dl
                .copied
                .checked_add(transfer_size)
                .ok_or_else(|| Error::MaximumTransferSizeExceeded)?;
            &bytes[..transfer_size as usize]
        } else {
            self.dl.eof = true;
            self.dl.copied = self
                .dl
                .copied
                .checked_add(len)
                .ok_or_else(|| Error::MaximumTransferSizeExceeded)?;
            bytes
        };

        let block_num = self.dl.block_num;
        self.dl.block_num = block_num
            .checked_add(1)
            .ok_or_else(|| Error::MaximumChunksExceeded)?;

        let res = self
            .dl
            .dfu
            .io
            .write_control(REQUEST_TYPE, DFU_DNLOAD, block_num, slice)?;

        Ok((next, Some(res)))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadCommandErase(u32);

impl From<DownloadCommandErase> for [u8; 5] {
    fn from(command: DownloadCommandErase) -> Self {
        let mut buffer = [0; 5];
        buffer[0] = 0x41;
        buffer[1..].copy_from_slice(&command.0.to_le_bytes());
        buffer
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadCommandSetAddress(u32);

impl From<DownloadCommandSetAddress> for [u8; 5] {
    fn from(command: DownloadCommandSetAddress) -> Self {
        let mut buffer = [0; 5];
        buffer[0] = 0x21;
        buffer[1..].copy_from_slice(&command.0.to_le_bytes());
        buffer
    }
}

#[must_use]
pub struct SetAddress<'dfu, 'mem, 'dl, IO: DfuIo> {
    dl: &'dl mut Loop<'dfu, 'mem, IO>,
}
