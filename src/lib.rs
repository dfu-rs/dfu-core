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
/// Generic synchronous implementation.
#[cfg(any(feature = "std", test))]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod sync;

use core::convert::TryFrom;

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
    /// Unknown DFU protocol
    UnknownProtocol,
    /// Failed to parse dfuse interface string
    InvalidInterfaceString,
    /// Failed to parse dfuse address from interface string
    #[cfg(any(feature = "std", test))]
    MemoryLayout(memory_layout::Error),
    /// Failed to parse dfuse address from interface string
    InvalidAddress,
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
    /// Dfuse Memory layout type
    type MemoryLayout: AsRef<memory_layout::mem>;

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

    /// Returns the protocol of the device
    fn protocol(&self) -> &DfuProtocol<Self::MemoryLayout>;

    /// Returns the functional descriptor of the device.
    fn functional_descriptor(&self) -> &functional_descriptor::FunctionalDescriptor;
}

/// The DFU protocol variant in use
pub enum DfuProtocol<M> {
    /// DFU 1.1
    Dfu,
    /// STM DFU extensions aka DfuSe
    Dfuse {
        /// Start memory address
        address: u32,
        /// Memory layout for flash
        memory_layout: M,
    },
}

#[cfg(any(feature = "std", test))]
impl DfuProtocol<memory_layout::MemoryLayout> {
    /// Create a DFU Protocol object from the interface string and DFU version
    pub fn new(interface_string: &str, version: (u8, u8)) -> Result<Self, Error> {
        match version {
            (0x1, 0x10) => Ok(DfuProtocol::Dfu),
            (0x1, 0x1a) => {
                let (rest, memory_layout) = interface_string
                    .rsplit_once('/')
                    .ok_or(Error::InvalidInterfaceString)?;
                let memory_layout = memory_layout::MemoryLayout::try_from(memory_layout)
                    .map_err(Error::MemoryLayout)?;
                let (_rest, address) =
                    rest.rsplit_once('/').ok_or(Error::InvalidInterfaceString)?;
                let address = address
                    .strip_prefix("0x")
                    .and_then(|s| u32::from_str_radix(s, 16).ok())
                    .ok_or(Error::InvalidAddress)?;
                Ok(DfuProtocol::Dfuse {
                    address,
                    memory_layout,
                })
            }
            _ => Err(Error::UnknownProtocol),
        }
    }
}

/// Use this struct to create state machines to make operations on the device.
pub struct DfuSansIo<IO> {
    io: IO,
}

impl<IO: DfuIo> DfuSansIo<IO> {
    /// Create an instance of [`DfuSansIo`].
    pub fn new(io: IO) -> Self {
        Self { io }
    }

    fn protocol(&self) -> &DfuProtocol<IO::MemoryLayout> {
        self.io.protocol()
    }

    /// Create a state machine to download the firmware into the device.
    pub fn download(
        &self,
        length: u32,
    ) -> Result<
        get_status::GetStatus<
            '_,
            IO,
            get_status::ClearStatus<'_, IO, get_status::GetStatus<'_, IO, download::Start<'_, IO>>>,
        >,
        Error,
    > {
        let (protocol, end_pos) = match self.protocol() {
            DfuProtocol::Dfu => (download::ProtocolData::Dfu, length),
            DfuProtocol::Dfuse {
                address,
                memory_layout,
                ..
            } => (
                download::ProtocolData::Dfuse(download::DfuseProtocolData {
                    address: *address,
                    erased_pos: *address,
                    address_set: false,
                    memory_layout: memory_layout.as_ref(),
                }),
                address.checked_add(length).ok_or(Error::NoSpaceLeft)?,
            ),
        };

        Ok(get_status::GetStatus {
            dfu: self,
            chained_command: get_status::ClearStatus {
                dfu: self,
                chained_command: get_status::GetStatus {
                    dfu: self,
                    chained_command: download::Start {
                        dfu: self,
                        protocol,
                        end_pos,
                    },
                },
            },
        })
    }

    /// Send a Detach request to the device
    pub fn detach(&self) -> Result<(), IO::Error> {
        const REQUEST_TYPE: u8 = 0b00100001;
        const DFU_DETACH: u8 = 0;
        self.io.write_control(REQUEST_TYPE, DFU_DETACH, 1000, &[])?;
        Ok(())
    }

    /// Reset the USB device
    pub fn usb_reset(&self) -> Result<IO::Reset, IO::Error> {
        self.io.usb_reset()
    }

    /// Consume the object and return its [`DfuIo`]
    pub fn into_inner(self) -> IO {
        self.io
    }

    /// Returns whether the device is will detach if requested
    pub fn will_detach(&self) -> bool {
        self.io.functional_descriptor().will_detach
    }

    /// Returns whether the device is manifestation tolerant
    pub fn manifestation_tolerant(&self) -> bool {
        self.io.functional_descriptor().manifestation_tolerant
    }
}

/// DFU Status.
///
/// Note: not the same as state!
#[derive(Debug, Clone, Copy, Eq, PartialEq, Display)]
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

impl From<u8> for Status {
    fn from(state: u8) -> Self {
        match state {
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
        }
    }
}

impl From<Status> for u8 {
    fn from(state: Status) -> Self {
        match state {
            Status::Ok => 0x00,
            Status::ErrTarget => 0x01,
            Status::ErrFile => 0x02,
            Status::ErrWrite => 0x03,
            Status::ErrErase => 0x04,
            Status::ErrCheckErased => 0x05,
            Status::ErrProg => 0x06,
            Status::ErrVerify => 0x07,
            Status::ErrAddress => 0x08,
            Status::ErrNotdone => 0x09,
            Status::ErrFirmware => 0x0a,
            Status::ErrVendor => 0x0b,
            Status::ErrUsbr => 0x0c,
            Status::ErrPor => 0x0d,
            Status::ErrUnknown => 0x0e,
            Status::ErrStalledpkt => 0x0f,
            Status::Other(other) => other,
        }
    }
}

/// DFU State.
///
/// Note: not the same as status!
#[derive(Debug, Clone, Copy, Eq, PartialEq, Display)]
pub enum State {
    /// Device is running its normal application.
    AppIdle,
    /// Device is running its normal application, has received the DFU_DETACH request, and is waiting for a USB reset.
    AppDetach,
    /// Device is operating in the DFU mode and is waiting for requests.
    DfuIdle,
    /// Device has received a block and is waiting for the host to solicit the status via DFU_GETSTATUS.
    DfuDnloadSync,
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

impl From<u8> for State {
    fn from(state: u8) -> Self {
        match state {
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
        }
    }
}

impl From<State> for u8 {
    fn from(state: State) -> Self {
        match state {
            State::AppIdle => 0,
            State::AppDetach => 1,
            State::DfuIdle => 2,
            State::DfuDnloadSync => 3,
            State::DfuDnbusy => 4,
            State::DfuDnloadIdle => 5,
            State::DfuManifestSync => 6,
            State::DfuManifest => 7,
            State::DfuManifestWaitReset => 8,
            State::DfuUploadIdle => 9,
            State::DfuError => 10,
            State::Other(other) => other,
        }
    }
}

impl State {
    // Not all possible state are, according to the spec, possible in the GetStatus result.. As
    // that's defined as the state the device will be as a result of the request, which may trigger
    // state transitions. Ofcourse some devices get this wrong... So this does a reasonable
    // converstion to what should have been the result...
    fn for_status(self) -> Self {
        match self {
            State::DfuManifestSync => State::DfuManifest,
            State::DfuDnloadSync => State::DfuDnbusy,
            _ => self,
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
    const _: [&dyn DfuIo<Read = (), Write = (), Reset = (), MemoryLayout = (), Error = Error>; 0] =
        [];
}
