//! Sans IO core library (traits and tools) for DFU.
#![no_std]
#![warn(missing_docs)]
#![allow(clippy::type_complexity)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(any(feature = "std", test))]
#[macro_use]
extern crate std;

/// Commands to detach the device.
pub mod detach;
/// Commands to download a firmware into the device.
pub mod download;
/// Functional descriptor.
pub mod functional_descriptor;
/// Commands to get the status of the device.
pub mod get_status;
/// Memory layout.
pub mod memory_layout;
/// Commands to reset the device.
pub mod reset;
/// Generic synchronous implementation.
#[cfg(any(feature = "std", test))]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod sync;

use displaydoc::Display;
#[cfg(any(feature = "std", test))]
use thiserror::Error;

#[derive(Debug, Display)]
#[cfg_attr(any(feature = "std", test), derive(Error))]
#[allow(missing_docs)]
pub enum Error {
    /// The size of the data being transferred exceeds the DFU capabilities.
    OutOfCapabilities,
    /// The device is in an invalid state (got: {got:?}, expected: {expected:?}).
    InvalidState { got: State, expected: State },
    /// Buffer size exceeds the maximum allowed.
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
    ResponseTooShort { got: usize, expected: usize },
    /// Device status is in error: {0}
    StatusError(Status),
    /// Device state is in error: {0}
    StateError(State),
}

/// Trait to implement lower level communication with a USB device.
pub trait DfuIo {
    /// Return type after calling [`Self::read_control`].
    type Read;
    /// Return type after calling [`Self::write_control`].
    type Write;
    /// Return type after calling [`Self::usb_reset`].
    type Reset;
    /// Error type.
    type Error: From<Error>;

    /// Read data using control transfer.
    fn read_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &mut [u8],
    ) -> Result<Self::Read, Self::Error>;

    /// Write data using control transfer.
    fn write_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &[u8],
    ) -> Result<Self::Write, Self::Error>;

    /// Triggers a USB reset.
    fn usb_reset(&self) -> Result<Self::Reset, Self::Error>;

    /// Returns the memory layout of the device.
    fn memory_layout(&self) -> &memory_layout::mem;

    /// Returns the functional descriptor of the device.
    fn functional_descriptor(&self) -> &functional_descriptor::FunctionalDescriptor;
}

/// Use this struct to create state machines to make operations on the device.
pub struct DfuSansIo<IO> {
    io: IO,
    address: u32,
}

impl<IO: DfuIo> DfuSansIo<IO> {
    /// Create an instance of [`DfuSansIo`].
    pub fn new(io: IO, address: u32) -> Self {
        Self { io, address }
    }

    /// Create a state machine to download the firmware into the device.
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

    /// Consume the object and return its [`DfuIo`] and address.
    pub fn into_parts(self) -> (IO, u32) {
        (self.io, self.address)
    }
}

/// DFU Status.
///
/// Note: not the same as state!
#[derive(Debug, Clone, Copy, PartialEq, Display)]
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
    ErrErase,
    /// Memory erase check failed.
    ErrCheckErased,
    /// Program memory function failed.
    ErrProg,
    /// Programmed memory failed verification.
    ErrVerify,
    /// Cannot program memory due to received address that is out of range.
    ErrAddress,
    /// Received DFU_DNLOAD with wLength = 0, but device does not think it has all of the data yet.
    ErrNotdone,
    /// Device's firmware is corrupt. It cannot return to run-time (non-DFU) operations.
    ErrFirmware,
    /// iString indicates a vendor-specific error.
    ErrVendor,
    /// Device detected unexpected USB reset signaling.
    ErrUsbr,
    /// Device detected unexpected power on reset.
    ErrPor,
    /// Something went wrong, but the device does not know what it was.
    ErrUnknown,
    /// Device stalled an unexpected request.
    ErrStalledpkt,
    /// Other ({0}).
    Other(u8),
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

/// DFU State.
///
/// Note: not the same as status!
#[derive(Debug, Clone, Copy, PartialEq, Display)]
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
    /// Other ({0}).
    Other(u8),
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

/// A trait for commands that be chained into another.
pub trait ChainedCommand {
    /// Type of the argument to pass with the command for chaining.
    type Arg;
    /// Type of the command after being chained.
    type Into;

    /// Chain this command into another.
    fn chain(self, arg: Self::Arg) -> Self::Into;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::v1::*;

    // ensure DfuIo can be made into an object
    const _: [&dyn DfuIo<Read = (), Write = (), Reset = (), Error = Error>; 0] = [];
}
