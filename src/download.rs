use super::*;

const REQUEST_TYPE: u8 = 0b00100001;
const DFU_DNLOAD: u8 = 1;

#[must_use]
pub struct Start<'dfu, IO: DfuIo> {
    pub(crate) dfu: &'dfu DfuSansIo<IO>,
    pub(crate) memory_layout: &'dfu memory_layout::mem,
    pub(crate) address: u32,
    pub(crate) end_pos: u32,
}

impl<'dfu, IO: DfuIo> ChainedCommand for Start<'dfu, IO> {
    type Arg = get_status::GetStatusMessage;
    type Into = Result<DownloadLoop<'dfu, IO>, Error>;

    fn chain(self, (_status, _poll_timeout, state, _index): Self::Arg) -> Self::Into {
        if state == State::DfuIdle {
            Ok(DownloadLoop {
                dfu: self.dfu,
                memory_layout: self.memory_layout,
                end_pos: self.end_pos,
                copied_pos: self.address,
                erased_pos: self.address,
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
pub struct DownloadLoop<'dfu, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<IO>,
    memory_layout: &'dfu memory_layout::mem,
    end_pos: u32,
    copied_pos: u32,
    erased_pos: u32,
    address_set: bool,
    block_num: u16,
    eof: bool,
}

impl<'dfu, IO: DfuIo> DownloadLoop<'dfu, IO> {
    pub fn next(self) -> Step<'dfu, IO> {
        if self.eof {
            Step::Break
        } else {
            if self.erased_pos < self.end_pos {
                Step::Erase(ErasePage {
                    dfu: self.dfu,
                    memory_layout: self.memory_layout,
                    end_pos: self.end_pos,
                    copied_pos: self.copied_pos,
                    erased_pos: self.erased_pos,
                    block_num: self.block_num,
                })
            } else if !self.address_set {
                Step::SetAddress(SetAddress {
                    dfu: self.dfu,
                    memory_layout: self.memory_layout,
                    end_pos: self.end_pos,
                    copied_pos: self.copied_pos,
                    erased_pos: self.erased_pos,
                    block_num: self.block_num,
                })
            } else {
                Step::DownloadChunk(DownloadChunk {
                    dfu: self.dfu,
                    memory_layout: self.memory_layout,
                    end_pos: self.end_pos,
                    copied_pos: self.copied_pos,
                    erased_pos: self.erased_pos,
                    block_num: self.block_num,
                })
            }
        }
    }
}

pub enum Step<'dfu, IO: DfuIo> {
    Break,
    Erase(ErasePage<'dfu, IO>),
    SetAddress(SetAddress<'dfu, IO>),
    DownloadChunk(DownloadChunk<'dfu, IO>),
}

#[must_use]
pub struct ErasePage<'dfu, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<IO>,
    memory_layout: &'dfu memory_layout::mem,
    end_pos: u32,
    copied_pos: u32,
    erased_pos: u32,
    block_num: u16,
}

impl<'dfu, IO: DfuIo> ErasePage<'dfu, IO> {
    pub fn erase(
        self,
    ) -> Result<
        (
            get_status::WaitState<'dfu, IO, DownloadLoop<'dfu, IO>>,
            IO::Write,
        ),
        IO::Error,
    > {
        let (page_size, rest_memory_layout) = self
            .memory_layout
            .split_first()
            .ok_or_else(|| Error::NoSpaceLeft)?;

        let next = get_status::WaitState {
            dfu: &self.dfu,
            state: State::DfuDnloadIdle,
            chained_command: DownloadLoop {
                dfu: self.dfu,
                memory_layout: rest_memory_layout,
                end_pos: self.end_pos,
                copied_pos: self.copied_pos,
                erased_pos: self
                    .erased_pos
                    .checked_add(*page_size)
                    .ok_or_else(|| Error::EraseLimitReached)?,
                block_num: self.block_num,
                address_set: false,
                eof: false,
            },
            end: false,
            poll_timeout: 0,
            in_manifest: false,
        };
        let res = self.dfu.io.write_control(
            REQUEST_TYPE,
            DFU_DNLOAD,
            0,
            &<[u8; 5]>::from(DownloadCommandErase(self.erased_pos)),
        )?;

        Ok((next, res))
    }
}

#[must_use]
pub struct SetAddress<'dfu, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<IO>,
    memory_layout: &'dfu memory_layout::mem,
    end_pos: u32,
    copied_pos: u32,
    erased_pos: u32,
    block_num: u16,
}

impl<'dfu, IO: DfuIo> SetAddress<'dfu, IO> {
    pub fn set_address(
        self,
    ) -> Result<
        (
            get_status::WaitState<'dfu, IO, DownloadLoop<'dfu, IO>>,
            IO::Write,
        ),
        IO::Error,
    > {
        let next = get_status::WaitState {
            dfu: &self.dfu,
            state: State::DfuDnloadIdle,
            chained_command: DownloadLoop {
                dfu: self.dfu,
                memory_layout: self.memory_layout,
                end_pos: self.end_pos,
                copied_pos: self.copied_pos,
                erased_pos: self.erased_pos,
                block_num: self.block_num,
                address_set: true,
                eof: false,
            },
            end: false,
            poll_timeout: 0,
            in_manifest: false,
        };
        let res = self.dfu.io.write_control(
            REQUEST_TYPE,
            DFU_DNLOAD,
            0,
            &<[u8; 5]>::from(DownloadCommandSetAddress(self.copied_pos)),
        )?;

        Ok((next, res))
    }
}

#[must_use]
pub struct DownloadChunk<'dfu, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<IO>,
    memory_layout: &'dfu memory_layout::mem,
    end_pos: u32,
    copied_pos: u32,
    erased_pos: u32,
    block_num: u16,
}

impl<'dfu, IO: DfuIo> DownloadChunk<'dfu, IO> {
    pub fn write(
        self,
        bytes: &[u8],
    ) -> Result<
        (
            get_status::WaitState<'dfu, IO, DownloadLoop<'dfu, IO>>,
            IO::Write,
        ),
        IO::Error,
    > {
        use std::convert::TryFrom;

        let len = u32::try_from(bytes.len())
            .map_err(|_| Error::BufferTooBig {
                got: bytes.len(),
                expected: u32::MAX as usize,
            })?
            .min(self.dfu.transfer_size);

        let next = get_status::WaitState {
            dfu: &self.dfu,
            state: State::DfuDnloadIdle,
            chained_command: DownloadLoop {
                dfu: self.dfu,
                memory_layout: self.memory_layout,
                end_pos: self.end_pos,
                copied_pos: self
                    .copied_pos
                    .checked_add(len)
                    .ok_or_else(|| Error::MaximumTransferSizeExceeded)?,
                erased_pos: self.erased_pos,
                block_num: self
                    .block_num
                    .checked_add(1)
                    .ok_or_else(|| Error::MaximumChunksExceeded)?,
                address_set: true,
                eof: bytes.is_empty(),
            },
            end: false,
            poll_timeout: 0,
            in_manifest: false,
        };
        let res = self.dfu.io.write_control(
            REQUEST_TYPE,
            DFU_DNLOAD,
            self.block_num,
            &bytes[..len as usize],
        )?;

        Ok((next, res))
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
