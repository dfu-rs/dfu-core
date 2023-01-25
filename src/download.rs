use super::*;

const REQUEST_TYPE: u8 = 0b00100001;
const DFU_DNLOAD: u8 = 1;

/// Starting point to download a firmware into a device.
#[must_use]
pub struct Start<'dfu, IO: DfuIo> {
    pub(crate) dfu: &'dfu DfuSansIo<IO>,
    pub(crate) end_pos: u32,
    pub(crate) protocol: ProtocolData<'dfu>,
}

impl<'dfu, IO: DfuIo> ChainedCommand for Start<'dfu, IO> {
    type Arg = get_status::GetStatusMessage;
    type Into = Result<DownloadLoop<'dfu, IO>, Error>;

    fn chain(
        self,
        get_status::GetStatusMessage {
            status: _,
            poll_timeout: _,
            state,
            index: _,
        }: Self::Arg,
    ) -> Self::Into {
        log::trace!("Starting download process");
        // TODO startup can be in AppIdle in which case the Detach-Attach process needs to be done
        if state == State::DfuIdle {
            let (block_num, copied_pos) = match self.protocol {
                ProtocolData::Dfu => (0, 0),
                ProtocolData::Dfuse(d) => (2, d.address),
            };
            Ok(DownloadLoop {
                dfu: self.dfu,
                end_pos: self.end_pos,
                copied_pos,
                protocol: self.protocol,
                block_num,
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

#[derive(Debug, Copy, Clone)]
pub(crate) struct DfuseProtocolData<'dfu> {
    pub address: u32,
    pub erased_pos: u32,
    pub address_set: bool,
    pub memory_layout: &'dfu memory_layout::mem,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum ProtocolData<'dfu> {
    Dfu,
    Dfuse(DfuseProtocolData<'dfu>),
}

/// Download loop.
#[must_use]
pub struct DownloadLoop<'dfu, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<IO>,
    protocol: ProtocolData<'dfu>,
    end_pos: u32,
    copied_pos: u32,
    block_num: u16,
    eof: bool,
}

impl<'dfu, IO: DfuIo> DownloadLoop<'dfu, IO> {
    /// Get the next step in the download loop.
    pub fn next(self) -> Step<'dfu, IO> {
        if self.eof {
            log::trace!("Download loop ended");

            let descriptor = self.dfu.io.functional_descriptor();
            // If the device won't detach itself, it expects to be reset by the host as there is
            // nothing more that can be done. Otherwise it is expected to detach by itself
            log::trace!("Device will detach? {}", descriptor.will_detach);
            return if !descriptor.manifestation_tolerant && !descriptor.will_detach {
                Step::UsbReset
            } else {
                Step::Break
            };
        }

        match self.protocol {
            ProtocolData::Dfuse(d) if d.erased_pos < self.end_pos => {
                log::trace!("Download loop: erase page");
                log::trace!("Erased position: {}", d.erased_pos);
                log::trace!("End position: {}", self.end_pos);
                Step::Erase(ErasePage {
                    dfu: self.dfu,
                    end_pos: self.end_pos,
                    copied_pos: self.copied_pos,
                    protocol: d,
                    block_num: self.block_num,
                })
            }
            ProtocolData::Dfuse(d) if !d.address_set => {
                log::trace!("Download loop: set address");
                Step::SetAddress(SetAddress {
                    dfu: self.dfu,
                    end_pos: self.end_pos,
                    copied_pos: self.copied_pos,
                    protocol: d,
                    block_num: self.block_num,
                })
            }
            _ => {
                log::trace!("Download loop: download chunk");
                Step::DownloadChunk(DownloadChunk {
                    dfu: self.dfu,
                    end_pos: self.end_pos,
                    copied_pos: self.copied_pos,
                    block_num: self.block_num,
                    protocol: self.protocol,
                })
            }
        }
    }
}

/// Download step in the loop.
#[allow(missing_docs)]
pub enum Step<'dfu, IO: DfuIo> {
    Break,
    UsbReset,
    Erase(ErasePage<'dfu, IO>),
    SetAddress(SetAddress<'dfu, IO>),
    DownloadChunk(DownloadChunk<'dfu, IO>),
}

/// Erase a memory page.
#[must_use]
pub struct ErasePage<'dfu, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<IO>,
    end_pos: u32,
    copied_pos: u32,
    protocol: DfuseProtocolData<'dfu>,
    block_num: u16,
}

impl<'dfu, IO: DfuIo> ErasePage<'dfu, IO> {
    /// Erase a memory page.
    pub fn erase(
        self,
    ) -> Result<
        (
            get_status::WaitState<'dfu, IO, DownloadLoop<'dfu, IO>>,
            IO::Write,
        ),
        IO::Error,
    > {
        let (&page_size, rest_memory_layout) = self
            .protocol
            .memory_layout
            .split_first()
            .ok_or(Error::NoSpaceLeft)?;
        log::trace!("Rest of memory layout: {:?}", rest_memory_layout);
        log::trace!("Page size: {:?}", page_size);

        let next_protocol = ProtocolData::Dfuse(DfuseProtocolData {
            erased_pos: self
                .protocol
                .erased_pos
                .checked_add(page_size)
                .ok_or(Error::EraseLimitReached)?,
            memory_layout: rest_memory_layout,
            ..self.protocol
        });
        let next = get_status::WaitState::new(
            self.dfu,
            State::DfuDnbusy,
            State::DfuDnloadIdle,
            DownloadLoop {
                dfu: self.dfu,
                protocol: next_protocol,
                end_pos: self.end_pos,
                copied_pos: self.copied_pos,
                block_num: self.block_num,
                eof: false,
            },
        );
        let res = self.dfu.io.write_control(
            REQUEST_TYPE,
            DFU_DNLOAD,
            0,
            &<[u8; 5]>::from(DownloadCommandErase(self.protocol.erased_pos)),
        )?;

        Ok((next, res))
    }
}

/// Set the address for download.
#[must_use]
pub struct SetAddress<'dfu, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<IO>,
    end_pos: u32,
    copied_pos: u32,
    protocol: DfuseProtocolData<'dfu>,
    block_num: u16,
}

impl<'dfu, IO: DfuIo> SetAddress<'dfu, IO> {
    /// Set the address for download.
    pub fn set_address(
        self,
    ) -> Result<
        (
            get_status::WaitState<'dfu, IO, DownloadLoop<'dfu, IO>>,
            IO::Write,
        ),
        IO::Error,
    > {
        let next_protocol = ProtocolData::Dfuse(DfuseProtocolData {
            address_set: true,
            ..self.protocol
        });

        let next = get_status::WaitState::new(
            self.dfu,
            State::DfuDnbusy,
            State::DfuDnloadIdle,
            DownloadLoop {
                dfu: self.dfu,
                end_pos: self.end_pos,
                copied_pos: self.copied_pos,
                protocol: next_protocol,
                block_num: self.block_num,
                eof: false,
            },
        );
        let res = self.dfu.io.write_control(
            REQUEST_TYPE,
            DFU_DNLOAD,
            0,
            &<[u8; 5]>::from(DownloadCommandSetAddress(self.copied_pos)),
        )?;

        Ok((next, res))
    }
}

/// Download a chunk of data into the device.
#[must_use]
pub struct DownloadChunk<'dfu, IO: DfuIo> {
    dfu: &'dfu DfuSansIo<IO>,
    end_pos: u32,
    copied_pos: u32,
    block_num: u16,
    protocol: ProtocolData<'dfu>,
}

impl<'dfu, IO: DfuIo> DownloadChunk<'dfu, IO> {
    /// Download a chunk of data into the device.
    pub fn download(
        self,
        bytes: &[u8],
    ) -> Result<
        (
            get_status::WaitState<'dfu, IO, DownloadLoop<'dfu, IO>>,
            IO::Write,
        ),
        IO::Error,
    > {
        let descriptor = self.dfu.io.functional_descriptor();
        let transfer_size = descriptor.transfer_size as u32;
        log::trace!("Transfer size: {}", transfer_size);
        let len = u32::try_from(bytes.len())
            .map_err(|_| Error::BufferTooBig {
                got: bytes.len(),
                expected: u32::MAX as usize,
            })?
            .min(transfer_size);
        log::trace!("Chunk length: {}", len);
        log::trace!("Copied position: {}", self.copied_pos);
        log::trace!("Block number: {}", self.block_num);

        let (intermediate, next_state) = if bytes.is_empty() {
            if descriptor.manifestation_tolerant {
                (State::DfuManifest, State::DfuIdle)
            } else {
                (State::DfuManifest, State::DfuManifest)
            }
        } else {
            (State::DfuDnbusy, State::DfuDnloadIdle)
        };

        let next = get_status::WaitState::new(
            self.dfu,
            intermediate,
            next_state,
            DownloadLoop {
                dfu: self.dfu,
                end_pos: self.end_pos,
                copied_pos: self
                    .copied_pos
                    .checked_add(len)
                    .ok_or(Error::MaximumTransferSizeExceeded)?,
                protocol: self.protocol,
                block_num: self
                    .block_num
                    .checked_add(1)
                    .ok_or(Error::MaximumChunksExceeded)?,
                eof: bytes.is_empty(),
            },
        );
        let res = self.dfu.io.write_control(
            REQUEST_TYPE,
            DFU_DNLOAD,
            self.block_num,
            &bytes[..len as usize],
        )?;

        Ok((next, res))
    }
}

/// Command to erase.
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

/// Command to set address to download.
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
