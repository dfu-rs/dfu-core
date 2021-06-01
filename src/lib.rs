pub mod control;
pub mod download;
pub mod get_status;
pub mod memory_layout;
pub mod sync;

use std::fmt;
use thiserror::Error;

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
}

pub trait DfuIo {
    type Read;
    type Write;
    type Error: From<Error>;

    fn read_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
    ) -> Result<Self::Read, Self::Error>;
    fn write_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &[u8],
    ) -> Result<Self::Write, Self::Error>;
}

pub struct DfuSansIo<'mem, IO> {
    io: IO,
    memory_layout: &'mem memory_layout::mem,
    transfer_size: u32,
}

impl<'mem, IO: DfuIo> DfuSansIo<'mem, IO> {
    pub fn new(io: IO, memory_layout: &'mem memory_layout::mem, transfer_size: u32) -> Self {
        Self {
            io,
            memory_layout,
            transfer_size,
        }
    }

    pub fn download<'dfu>(
        &'dfu self,
        address: u32,
        length: impl Into<Option<usize>>,
    ) -> get_status::ClearStatus<
        'dfu,
        'mem,
        IO,
        get_status::GetStatus<'dfu, 'mem, IO, download::Start<'dfu, 'mem, IO>>,
    > {
        get_status::ClearStatus {
            dfu: self,
            chained_command: get_status::GetStatus {
                dfu: self,
                chained_command: download::Start {
                    dfu: self,
                    memory_layout: self.memory_layout,
                    address,
                    length: length.into(),
                },
            },
        }
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
            }
        )
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
            },
        )
    }
}

pub trait ChainedCommand {
    type Arg;
    type Into;

    fn chain(self, arg: Self::Arg) -> Self::Into;
}

pub trait ChainedCommandBytes {
    type Into;

    fn chain(self, bytes: &[u8]) -> Self::Into;
}

pub mod dfu_libusb {
    use crate as dfu_core;
    use thiserror::Error;

    pub type Dfu<'usb, 'mem> = dfu_core::sync::DfuSync<'mem, DfuLibusb<'usb>>;

    #[derive(Debug, Error)]
    pub enum Error {
        #[error("Could not find device or an error occurred")]
        CouldNotOpenDevice,
        #[error(transparent)]
        Dfu(#[from] dfu_core::Error),
    }

    pub struct DfuLibusb<'usb> {
        usb: libusb::DeviceHandle<'usb>,
        buffer: bytes::BytesMut,
        memory_layout: dfu_core::memory_layout::MemoryLayout,
    }

    impl<'usb> dfu_core::DfuIo for DfuLibusb<'usb> {
        type Read = ();
        type Write = usize;
        type Error = Error;

        #[allow(unused_variables)]
        fn read_control(
            &self,
            request_type: u8,
            request: u8,
            value: u16,
        ) -> Result<Self::Read, Self::Error> {
            todo!()
        }

        #[allow(unused_variables)]
        fn write_control(
            &self,
            request_type: u8,
            request: u8,
            value: u16,
            buffer: &[u8],
        ) -> Result<Self::Write, Self::Error> {
            todo!()
        }
    }

    impl<'usb> DfuLibusb<'usb> {
        pub fn open<'mem>(
            context: &'usb libusb::Context,
            vid: u16,
            pid: u16,
        ) -> Result<Dfu<'usb, 'mem>, Error> {
            let usb = context
                .open_device_with_vid_pid(vid, pid)
                .ok_or(Error::CouldNotOpenDevice)?;
            let transfer_size: u32 = todo!();
            let buffer = bytes::BytesMut::with_capacity(transfer_size as usize);
            let memory_layout: dfu_core::memory_layout::MemoryLayout = todo!();
            let io = DfuLibusb {
                usb,
                buffer,
                memory_layout,
            };

            Ok(dfu_core::sync::DfuSync::new(
                io,
                io.memory_layout.as_ref(),
                transfer_size,
            ))
        }
    }

    #[allow(unused_variables)]
    pub fn test() -> Result<(), Box<dyn std::error::Error>> {
        let context = libusb::Context::new()?;
        let device: Dfu = DfuLibusb::open(&context, 0x0483, 0xdf11)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate as dfu_core;

    #[test]
    #[should_panic]
    fn ensure_io_can_be_made_into_an_object() {
        let boxed: Box<dyn dfu_core::DfuIo<Read = (), Write = (), Error = dfu_core::Error>> =
            todo!();
    }
}
