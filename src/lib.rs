//! Sans IO core library (traits and tools) for DFU.

#![no_std]
#![warn(missing_docs)]
#![allow(clippy::type_complexity)]

#[cfg(any(feature = "std", test))]
#[macro_use]
extern crate std;

/// Module for handling the DFU_DETACH request.
pub mod detach;
/// Module for the logic of writing a firmware to the device.
pub mod download;
/// Module for representing and parsing the DFU functional descriptor.
pub mod functional_descriptor;
/// Module for handling the status of the device.
pub mod get_status;
/// Module for representing and parsing the memory layout of the device.
pub mod memory_layout;
/// Module for resetting the USB device.
pub mod reset;
/// Basic synchronous implementation of DFU. (Requires `std`.)
#[cfg(any(feature = "std", test))]
pub mod sync;

use core::fmt;
use displaydoc::Display;
#[cfg(any(feature = "std", test))]
use thiserror::Error;

/// Error that might happen during the execution of the commands.
#[derive(Debug, Display)]
#[cfg_attr(feature = "std", derive(Error))]
pub enum Error {
    /// The device is in an invalid state (got: {got:?}, expected: {expected:?}).
    #[allow(missing_docs)]
    InvalidState { got: State, expected: State },
    /// Buffer size exceeds the maximum allowed.
    #[allow(missing_docs)]
    BufferTooBig { got: usize, expected: usize },
    /// Maximum transfer size exceeded.
    MaximumTransferSizeExceeded,
    /// Erasing limit reached.
    EraseLimitReached,
    /// Maximum number of chunks exceeded.
    MaximumChunksExceeded,
    /// Not enough space on device.
    NoSpaceLeft,
    /// Unrecognized status code: {0}
    UnrecognizedStatusCode(u8),
    /// Unrecognized state code: {0}
    UnrecognizedStateCode(u8),
    /// Device response is too short (got: {got:?}, expected: {expected:?}).
    #[allow(missing_docs)]
    ResponseTooShort { got: usize, expected: usize },
    /// Device status is in error: {0}
    StatusError(Status),
    /// Device state is in error: {0}
    StateError(State),
}

/// A trait that can be made into an object that provides the IO to this library logic.
pub trait DfuIo {
    /// Return type after reading a control request.
    type Read;
    /// Return type after writing a control request.
    type Write;
    /// Return type after triggering a USB reset on the device.
    type Reset;
    /// Error type of this implementation.
    type Error: From<Error>;

    /// Read control request.
    fn read_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &mut [u8],
    ) -> Result<Self::Read, Self::Error>;

    /// Write control request.
    fn write_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &[u8],
    ) -> Result<Self::Write, Self::Error>;

    /// Triggers a USB reset on the device.
    fn usb_reset(&self) -> Result<Self::Reset, Self::Error>;

    /// Retrieve the memory layout of the device.
    fn memory_layout(&self) -> &memory_layout::mem;

    /// Retrieve the functional descriptor of the device.
    fn functional_descriptor(&self) -> &functional_descriptor::FunctionalDescriptor;
}

/// A struct that allows the developer to do the DFU logic using a state machine (can be async or
/// sync).
pub struct DfuSansIo<IO> {
    io: IO,
    address: u32,
}

impl<IO: DfuIo> DfuSansIo<IO> {
    // TODO address should probably be moved to download()
    /// Create a new instances based on a `DfuIo` object and an address (where to write/read).
    pub fn new(io: IO, address: u32) -> Self {
        Self { io, address }
    }

    /// Creates an state machine that can be executed to write a firmware to the device.
    pub fn download(
        &self,
        length: u32,
    ) -> Result<
        get_status::ClearStatus<'_, IO, get_status::GetStatus<'_, IO, download::Start<'_, IO>>>,
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
/// DFU statuses.
pub enum Status {
    /// No error condition is present.
    Ok,
    /// File is not targeted for use by this device.
    ErrTarget,
    /// File is for this device but fails some vendor-specific verification test.
    ErrFile,
    /// Device is unable to write memory.
    ErrWrite,
    /// Memory erase function failed.
    ErrCheckErased,
    /// Memory erase check failed.
    ErrProg,
    /// Program memory function failed.
    ErrVerify,
    /// Programmed memory failed verification.
    ErrAddress,
    /// Cannot program memory due to received address that is out of range.
    ErrNotdone,
    /// Received DFU_DNLOAD with wLength = 0, but device does not think it has all of the data yet.
    ErrFirmware,
    /// Device's firmware is corrupt. It cannot return to run-time (non-DFU) operations.
    ErrVendor,
    /// iString indicates a vendor-specific error.
    ErrUsbr,
    /// Device detected unexpected USB reset signaling.
    ErrPor,
    /// Device detected unexpected power on reset.
    ErrErase,
    /// Something went wrong, but the device does not know what it was.
    ErrUnknown,
    /// Device stalled an unexpected request.
    ErrStalledpkt,
    /// Other status (not recognized).
    Other(u8),
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use Status::*;

        // TODO those messages should probably be re-wrote as they are intended to the user.
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

/// DFU states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    /// Device is running its normal application.
    AppIdle,
    /// Device is running its normal application, has received the DFU_DETACH request, and is waiting for a USB reset.
    AppDetach,
    /// Device is operating in the DFU mode and is waiting for requests.
    DfuIdle,
    /// Device has received a block and is waiting for the host to solicit the status via DFU_GETSTATUS.
    DfuUnloadSync,
    /// Device is programming a control-write block into its nonvolatile memories.
    DfuDnbusy,
    /// Device is processing a download operation.  Expecting DFU_DNLOAD requests.
    DfuDnloadIdle,
    /// Device has received the final block of firmware from the host and is waiting for receipt of DFU_GETSTATUS to begin the Manifestation phase; or device has completed the Manifestation phase and is waiting for receipt of DFU_GETSTATUS.  (Devices that can enter this state after the Manifestation phase set bmAttributes bit bitManifestationTolerant to 1.)
    DfuManifestSync,
    /// Device is in the Manifestation phase.  (Not all devices will be able to respond to DFU_GETSTATUS when in this state.)
    DfuManifest,
    /// Device has programmed its memories and is waiting for a USB reset or a power on reset.  (Devices that must enter this state clear bitManifestationTolerant to 0.)
    DfuManifestWaitReset,
    /// The device is processing an upload operation.  Expecting DFU_UPLOAD requests.
    DfuUploadIdle,
    /// An error has occurred. Awaiting the DFU_CLRSTATUS request.
    DfuError,
    /// Other state (not recognized).
    Other(u8),
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use State::*;

        // TODO those messages should probably be re-wrote as they are intended to the user.
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

/// Trait that allows chaining a command to another command by taking ownership of the original
/// command and transforming it to another.
pub trait ChainedCommand {
    /// Argument to passe to the transformation function.
    type Arg;
    /// Command to transform into.
    type Into;

    /// This function is run to transform a command (`self`) to another (`Self::Into`).
    fn chain(self, arg: Self::Arg) -> Self::Into;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::*;

    // ensure DfuIo can be made into an object
    const _: [&dyn DfuIo<Read = (), Write = (), Reset = (), Error = Error>; 0] = [];
}
