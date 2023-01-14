use super::*;
use bytes::Buf;
use pretty_hex::PrettyHex;

const REQUEST_TYPE: u8 = 0b00100001;
const DFU_GETSTATUS: u8 = 3;
const DFU_CLRSTATUS: u8 = 4;

/// Get status message.
pub struct GetStatusMessage {
    /// Status.
    pub status: Status,
    /// Poll timeout.
    pub poll_timeout: u64,
    /// State.
    pub state: State,
    /// Index.
    pub index: u8,
}

/// Command that queries the status of the device.
#[must_use]
pub struct GetStatus<'dfu, IO: DfuIo, T: ChainedCommand<Arg = GetStatusMessage>> {
    pub(crate) dfu: &'dfu DfuSansIo<IO>,
    pub(crate) chained_command: T,
}

impl<'dfu, IO: DfuIo, T: ChainedCommand<Arg = GetStatusMessage>> GetStatus<'dfu, IO, T> {
    /// Query the status of the device.
    pub fn get_status(self, buffer: &mut [u8]) -> Result<(GetStatusRecv<T>, IO::Read), IO::Error> {
        debug_assert!(buffer.len() >= 6);
        let next = GetStatusRecv {
            chained_command: self.chained_command,
        };
        let res = self
            .dfu
            .io
            .read_control(REQUEST_TYPE, DFU_GETSTATUS, 0, buffer)?;
        Ok((next, res))
    }
}

/// Read status after getting it from the device.
#[must_use]
pub struct GetStatusRecv<T: ChainedCommand<Arg = GetStatusMessage>> {
    chained_command: T,
}

// TODO: this impl does not use ChainedCommand because the argument has an anonymous lifetime.
impl<T: ChainedCommand<Arg = GetStatusMessage>> GetStatusRecv<T> {
    /// Chain this command into another.
    pub fn chain(self, mut bytes: &[u8]) -> Result<T::Into, Error> {
        log::trace!("Received device status: {}", bytes.hex_dump());
        if bytes.len() < 6 {
            return Err(Error::ResponseTooShort {
                got: bytes.len(),
                expected: 6,
            });
        }

        let status = match bytes.get_u8() {
            0x00 => Status::Ok,
            0x01 => Status::ErrTarget,
            0x02 => Status::ErrFile,
            0x03 => Status::ErrWrite,
            0x04 => Status::ErrErase,
            0x05 => Status::ErrCheckErased,
            0x06 => Status::ErrProg,
            0x07 => Status::ErrVerify,
            0x08 => Status::ErrAddress,
            0x09 => Status::ErrNotdone,
            0x0a => Status::ErrFirmware,
            0x0b => Status::ErrVendor,
            0x0c => Status::ErrUsbr,
            0x0d => Status::ErrPor,
            0x0e => Status::ErrUnknown,
            0x0f => Status::ErrStalledpkt,
            other => Status::Other(other),
        };
        log::trace!("Device status: {:?}", status);
        let poll_timeout = bytes.get_uint_le(3);
        log::trace!("Poll timeout: {}", poll_timeout);
        let state = match bytes.get_u8() {
            0 => State::AppIdle,
            1 => State::AppDetach,
            2 => State::DfuIdle,
            3 => State::DfuDnloadSync,
            4 => State::DfuDnbusy,
            5 => State::DfuDnloadIdle,
            6 => State::DfuManifestSync,
            7 => State::DfuManifest,
            8 => State::DfuManifestWaitReset,
            9 => State::DfuUploadIdle,
            10 => State::DfuError,
            other => State::Other(other),
        };
        log::trace!("Device state: {:?}", state);
        let i_string = bytes.get_u8();
        log::trace!("Device i string: {:#x}", i_string);

        Ok(self.chained_command.chain(GetStatusMessage {
            status,
            poll_timeout,
            state,
            index: i_string,
        }))
    }
}

/// Command that clears the status of the device.
#[must_use]
pub struct ClearStatus<'dfu, IO: DfuIo, T> {
    pub(crate) dfu: &'dfu DfuSansIo<IO>,
    pub(crate) chained_command: T,
}

impl<'dfu, IO: DfuIo, T> ChainedCommand for ClearStatus<'dfu, IO, T> {
    type Arg = get_status::GetStatusMessage;
    type Into = Result<(T, Option<IO::Write>), IO::Error>;

    /// Clear the status of the device.
    fn chain(
        self,
        get_status::GetStatusMessage {
            status: _,
            poll_timeout: _,
            state,
            index: _,
        }: Self::Arg,
    ) -> Result<(T, Option<IO::Write>), IO::Error> {
        let next = self.chained_command;
        if state == State::DfuError {
            log::trace!("Device is in error state, clearing status...");
            let res = self
                .dfu
                .io
                .write_control(REQUEST_TYPE, DFU_CLRSTATUS, 0, &[])?;

            Ok((next, Some(res)))
        } else {
            log::trace!("Device is not in error state, skip clearing status");
            Ok((next, None))
        }
    }
}

/// Wait for the device to enter a specific state.
#[must_use]
pub struct WaitState<'dfu, IO: DfuIo, T> {
    dfu: &'dfu DfuSansIo<IO>,
    state: State,
    chained_command: T,
    end: bool,
    poll_timeout: u64,
    in_manifest: bool,
}

/// A step when waiting for a state.
#[allow(missing_docs)]
pub enum Step<'dfu, IO: DfuIo, T> {
    Break(T),
    /// The state has not been reached and the status of the device must be queried.
    Wait(GetStatus<'dfu, IO, WaitState<'dfu, IO, T>>, u64),
    /// The device is in manifest state and might require a USB reset.
    ManifestWaitReset(Option<reset::UsbReset<'dfu, IO, ()>>),
}

impl<'dfu, IO: DfuIo, T> WaitState<'dfu, IO, T> {
    /// Create a new instance of [`WaitState`].
    pub fn new(dfu: &'dfu DfuSansIo<IO>, state: State, chained_command: T) -> Self {
        Self {
            dfu,
            state,
            chained_command,
            end: false,
            poll_timeout: 0,
            in_manifest: false,
        }
    }

    /// Returns the next command after waiting for a state.
    pub fn next(self) -> Step<'dfu, IO, T> {
        let func_desc = self.dfu.io.functional_descriptor();

        if self.end {
            log::trace!("Device state OK");
            Step::Break(self.chained_command)
        } else if self.in_manifest && !func_desc.manifestation_tolerant {
            log::trace!("Device in state manifest");
            log::trace!("Device will detach? {}", func_desc.will_detach);
            Step::ManifestWaitReset((!func_desc.will_detach).then(|| reset::UsbReset {
                dfu: self.dfu,
                chained_command: (),
            }))
        } else {
            let poll_timeout = self.poll_timeout;
            log::trace!(
                "Waiting for device state: {:?} (poll timeout: {})",
                self.state,
                poll_timeout,
            );

            Step::Wait(
                GetStatus {
                    dfu: self.dfu,
                    chained_command: self,
                },
                poll_timeout,
            )
        }
    }
}

impl<'dfu, IO: DfuIo, T> ChainedCommand for WaitState<'dfu, IO, T> {
    type Arg = GetStatusMessage;
    type Into = Self;

    fn chain(
        self,
        GetStatusMessage {
            status: _,
            poll_timeout,
            state,
            index: _,
        }: Self::Arg,
    ) -> Self::Into {
        log::trace!("Device state: {:?}", state);
        WaitState {
            dfu: self.dfu,
            chained_command: self.chained_command,
            state: self.state,
            end: state == self.state,
            poll_timeout,
            in_manifest: state == State::DfuManifest,
        }
    }
}
