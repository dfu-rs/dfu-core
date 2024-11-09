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
pub struct GetStatus<T: ChainedCommand<Arg = GetStatusMessage>> {
    pub(crate) chained_command: T,
}

impl<T: ChainedCommand<Arg = GetStatusMessage>> GetStatus<T> {
    /// Query the status of the device.
    pub fn get_status(self, buffer: &mut [u8]) -> (GetStatusRecv<T>, UsbReadControl) {
        debug_assert!(buffer.len() >= 6);
        let next = GetStatusRecv {
            chained_command: self.chained_command,
        };

        let control = UsbReadControl::new(REQUEST_TYPE, DFU_GETSTATUS, 0, buffer);
        (next, control)
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

        let status = bytes.get_u8().into();
        log::trace!("Device status: {:?}", status);
        let poll_timeout = bytes.get_uint_le(3);
        log::trace!("Poll timeout: {}", poll_timeout);
        let state: State = bytes.get_u8().into();
        let state = state.for_status();
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
pub struct ClearStatus<T> {
    pub(crate) chained_command: T,
}

impl<T> ChainedCommand for ClearStatus<T> {
    type Arg = get_status::GetStatusMessage;
    type Into = (T, Option<UsbWriteControl<[u8; 0]>>);

    /// Clear the status of the device.
    fn chain(
        self,
        get_status::GetStatusMessage {
            status: _,
            poll_timeout: _,
            state,
            index: _,
        }: Self::Arg,
    ) -> (T, Option<UsbWriteControl<[u8; 0]>>) {
        let next = self.chained_command;
        if state == State::DfuError {
            log::trace!("Device is in error state, clearing status...");
            let control = UsbWriteControl::new(REQUEST_TYPE, DFU_CLRSTATUS, 0, []);

            (next, Some(control))
        } else {
            log::trace!("Device is not in error state, skip clearing status");
            (next, None)
        }
    }
}

/// Wait for the device to enter a specific state.
#[must_use]
pub struct WaitState<T> {
    intermediate: State,
    state: State,
    chained_command: T,
    end: bool,
    poll_timeout: u64,
}

/// A step when waiting for a state.
#[allow(missing_docs)]
pub enum Step<T> {
    Break(T),
    /// The state has not been reached and the status of the device must be queried.
    Wait(GetStatus<WaitState<T>>, u64),
}

impl<T> WaitState<T> {
    /// Create a new instance of [`WaitState`].
    pub fn new(intermediate: State, state: State, chained_command: T) -> Self {
        Self {
            intermediate,
            state,
            chained_command,
            end: false,
            poll_timeout: 0,
        }
    }

    /// Returns the next command after waiting for a state.
    pub fn next(self) -> Step<T> {
        if self.end {
            log::trace!("Device state OK");
            Step::Break(self.chained_command)
        } else {
            let poll_timeout = self.poll_timeout;
            log::trace!(
                "Waiting for device state: {:?} (poll timeout: {})",
                self.state,
                poll_timeout,
            );

            Step::Wait(
                GetStatus {
                    chained_command: self,
                },
                poll_timeout,
            )
        }
    }
}

impl<T> ChainedCommand for WaitState<T> {
    type Arg = GetStatusMessage;
    type Into = Result<Self, Error>;

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
        if state == self.state || state == self.intermediate {
            Ok(WaitState {
                chained_command: self.chained_command,
                intermediate: self.intermediate,
                state: self.state,
                end: state == self.state,
                poll_timeout,
            })
        } else {
            Err(Error::InvalidState {
                got: state,
                expected: self.intermediate,
            })
        }
    }
}
