#![no_std]

#[cfg(any(feature = "std", test))]
#[macro_use]
extern crate std;

pub mod detach;
pub mod download;
pub mod functional_descriptor;
pub mod get_status;
pub mod memory_layout;
pub mod reset;
#[cfg(any(feature = "std", test))]
pub mod sync;

use core::fmt;
#[cfg(any(feature = "std", test))]
use thiserror::Error;

#[cfg(any(feature = "std", test))]
#[derive(Debug, Error)]
pub enum Error {
    #[error("The device is in an invalid state (got: {got:?}, expected: {expected:?}).")]
    InvalidState { got: State, expected: State },
    #[error("Buffer size exceeds the maximum allowed.")]
    BufferTooBig { got: usize, expected: usize },
    #[error("Maximum transfer size exceeded.")]
    MaximumTransferSizeExceeded,
    #[error("Erasing limit reached.")]
    EraseLimitReached,
    #[error("Maximum number of chunks exceeded.")]
    MaximumChunksExceeded,
    #[error("Not enough space on device.")]
    NoSpaceLeft,
    #[error("Unrecognized status code: {0}")]
    UnrecognizedStatusCode(u8),
    #[error("Unrecognized state code: {0}")]
    UnrecognizedStateCode(u8),
    #[error("Device response is too short (got: {got:?}, expected: {expected:?}).")]
    ResponseTooShort { got: usize, expected: usize },
    #[error("Device status is in error: {0}")]
    StatusError(Status),
    #[error("Device state is in error: {0}")]
    StateError(State),
}

#[cfg(not(any(feature = "std", test)))]
#[derive(Debug)]
pub enum Error {
    InvalidState { got: State, expected: State },
    BufferTooBig { got: usize, expected: usize },
    MaximumTransferSizeExceeded,
    EraseLimitReached,
    MaximumChunksExceeded,
    NoSpaceLeft,
    UnrecognizedStatusCode(u8),
    UnrecognizedStateCode(u8),
    ResponseTooShort { got: usize, expected: usize },
    StatusError(Status),
    StateError(State),
}

pub trait DfuIo {
    type Read;
    type Write;
    type Reset;
    type Error: From<Error>;

    fn read_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &mut [u8],
    ) -> Result<Self::Read, Self::Error>;

    fn write_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &[u8],
    ) -> Result<Self::Write, Self::Error>;

    fn usb_reset(&self) -> Result<Self::Reset, Self::Error>;

    fn memory_layout(&self) -> &memory_layout::mem;

    fn functional_descriptor(&self) -> &functional_descriptor::FunctionalDescriptor;
}

pub struct DfuSansIo<IO> {
    io: IO,
    address: u32,
}

impl<IO: DfuIo> DfuSansIo<IO> {
    pub fn new(io: IO, address: u32) -> Self {
        Self { io, address }
    }

    pub fn download<'dfu>(
        &'dfu self,
        length: u32,
    ) -> Result<
        get_status::ClearStatus<
            'dfu,
            IO,
            get_status::GetStatus<'dfu, IO, download::Start<'dfu, IO>>,
        >,
        Error,
    > {
        Ok(get_status::ClearStatus {
            dfu: self,
            chained_command: get_status::GetStatus {
                dfu: self,
                chained_command: download::Start {
                    dfu: self,
                    memory_layout: self.io.memory_layout(),
                    address: self.address,
                    end_pos: self.address.checked_add(length).ok_or(Error::NoSpaceLeft)?,
                },
            },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Ok,
    ErrTarget,
    ErrFile,
    ErrWrite,
    ErrErase,
    ErrCheckErased,
    ErrProg,
    ErrVerify,
    ErrAddress,
    ErrNotdone,
    ErrFirmware,
    ErrVendor,
    ErrUsbr,
    ErrPor,
    ErrUnknown,
    ErrStalledpkt,
    Other(u8),
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Status::*;

        write!(
            f,
            "{}",
            match self {
                Ok => "No error condition is present.",
                ErrTarget => "File is not targeted for use by this device.",
                ErrFile => "File is for this device but fails some vendor-specific verification test.",
                ErrWrite => "Device is unable to write memory.",
                ErrErase => "Memory erase function failed.",
                ErrCheckErased => "Memory erase check failed.",
                ErrProg => "Program memory function failed.",
                ErrVerify => "Programmed memory failed verification.",
                ErrAddress => "Cannot program memory due to received address that is out of range.",
                ErrNotdone => "Received DFU_DNLOAD with wLength = 0, but device does not think it has all of the data yet.",
                ErrFirmware => "Device's firmware is corrupt. It cannot return to run-time (non-DFU) operations.",
                ErrVendor => "iString indicates a vendor-specific error.",
                ErrUsbr => "Device detected unexpected USB reset signaling.",
                ErrPor => "Device detected unexpected power on reset.",
                ErrUnknown => "Something went wrong, but the device does not know what it was.",
                ErrStalledpkt => "Device stalled an unexpected request.",
                // TODO format code
                Other(_) => "Other status",
            }
        )
    }
}

impl Status {
    pub(crate) fn raise_error(&self) -> Result<(), Error> {
        if !matches!(self, Status::Ok | Status::Other(_)) {
            Err(Error::StatusError(*self))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    AppIdle,
    AppDetach,
    DfuIdle,
    DfuUnloadSync,
    DfuDnbusy,
    DfuDnloadIdle,
    DfuManifestSync,
    DfuManifest,
    DfuManifestWaitReset,
    DfuUploadIdle,
    DfuError,
    Other(u8),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use State::*;

        write!(
            f,
            "{}",
            match self {
                AppIdle => "Device is running its normal application.",
                AppDetach => "Device is running its normal application, has received the DFU_DETACH request, and is waiting for a USB reset.",
                DfuIdle => "Device is operating in the DFU mode and is waiting for requests.",
                DfuUnloadSync => "Device has received a block and is waiting for the host to solicit the status via DFU_GETSTATUS.",
                DfuDnbusy => "Device is programming a control-write block into its nonvolatile memories.",
                DfuDnloadIdle => "Device is processing a download operation.  Expecting DFU_DNLOAD requests.",
                DfuManifestSync => "Device has received the final block of firmware from the host and is waiting for receipt of DFU_GETSTATUS to begin the Manifestation phase; or device has completed the Manifestation phase and is waiting for receipt of DFU_GETSTATUS.  (Devices that can enter this state after the Manifestation phase set bmAttributes bit bitManifestationTolerant to 1.)",
                DfuManifest => "Device is in the Manifestation phase.  (Not all devices will be able to respond to DFU_GETSTATUS when in this state.)",
                DfuManifestWaitReset => "Device has programmed its memories and is waiting for a USB reset or a power on reset.  (Devices that must enter this state clear bitManifestationTolerant to 0.)",
                DfuUploadIdle => "The device is processing an upload operation.  Expecting DFU_UPLOAD requests.",
                DfuError => "An error has occurred. Awaiting the DFU_CLRSTATUS request.",
                // TODO format code
                Other(_) => "Other state",
            },
        )
    }
}

impl State {
    pub(crate) fn raise_error(&self) -> Result<(), Error> {
        if matches!(self, State::DfuError) {
            Err(Error::StateError(*self))
        } else {
            Ok(())
        }
    }
}

pub trait ChainedCommand {
    type Arg;
    type Into;

    fn chain(self, arg: Self::Arg) -> Self::Into;
}

#[cfg(test)]
mod tests {
    use std::prelude::v1::*;
    use crate as dfu_core;

    #[test]
    #[ignore]
    fn ensure_io_can_be_made_into_an_object() {
        let _boxed: Box<
            dyn dfu_core::DfuIo<Read = (), Write = (), Reset = (), Error = dfu_core::Error>,
        > = unreachable!();
    }
}
