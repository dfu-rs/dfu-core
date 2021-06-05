pub mod download;
pub mod get_status;
pub mod memory_layout;
pub mod reset;
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
}

pub struct DfuSansIo<IO> {
    io: IO,
    address: u32,
    transfer_size: u32,
}

impl<IO: DfuIo> DfuSansIo<IO> {
    pub fn new(io: IO, address: u32, transfer_size: u32) -> Self {
        Self {
            io,
            address,
            transfer_size,
        }
    }

    pub fn download<'dfu>(
        &'dfu self,
        length: u32,
    ) -> Result<
        reset::UsbReset<
            'dfu,
            IO,
            get_status::ClearStatus<
                'dfu,
                IO,
                get_status::GetStatus<'dfu, IO, download::Start<'dfu, IO>>,
            >,
        >,
        Error,
    > {
        Ok(reset::UsbReset {
            dfu: self,
            chained_command: get_status::ClearStatus {
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
        if !matches!(self, Status::Ok) {
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
    use crate as dfu_core;
    use std::cell::RefCell;
    use thiserror::Error;

    pub type Dfu<'usb> = dfu_core::sync::DfuSync<DfuLibusb<'usb>, Error>;

    #[derive(Debug, Error)]
    pub enum Error {
        #[error("Could not find device or an error occurred")]
        CouldNotOpenDevice,
        #[error(transparent)]
        Dfu(#[from] dfu_core::Error),
        #[error(transparent)]
        Io(#[from] std::io::Error),
        #[error("Could not parse memory layout: {0}")]
        MemoryLayout(String),
        #[error("libusb: {0}")]
        LibUsb(#[from] libusb::Error),
    }

    pub struct DfuLibusb<'usb> {
        usb: RefCell<libusb::DeviceHandle<'usb>>,
        memory_layout: dfu_core::memory_layout::MemoryLayout,
        timeout: std::time::Duration,
        index: u16,
    }

    impl<'usb> dfu_core::DfuIo for DfuLibusb<'usb> {
        type Read = usize;
        type Write = usize;
        type Reset = ();
        type Error = Error;

        #[allow(unused_variables)]
        fn read_control(
            &self,
            request_type: u8,
            request: u8,
            value: u16,
            buffer: &mut [u8],
        ) -> Result<Self::Read, Self::Error> {
            // TODO: do or do not?
            let request_type = request_type | 0x80;
            //println!("read {:b} {} {}", request_type, request, value);
            let res = self.usb.borrow().read_control(
                request_type,
                request,
                value,
                self.index,
                buffer,
                self.timeout,
            );
            //println!("read response {:x?}", &buffer);
            assert!(
                !matches!(res, Err(libusb::Error::InvalidParam)),
                "invalid param: {:08b} {:?}",
                request_type,
                res,
            );
            Ok(res?)
        }

        #[allow(unused_variables)]
        fn write_control(
            &self,
            request_type: u8,
            request: u8,
            value: u16,
            buffer: &[u8],
        ) -> Result<Self::Write, Self::Error> {
            //println!("write {:b} {} {} {:x?}", request_type, request, value, buffer);
            let res = self.usb.borrow().write_control(
                request_type,
                request,
                value,
                self.index,
                buffer,
                self.timeout,
            );
            assert!(
                !matches!(res, Err(libusb::Error::InvalidParam)),
                "invalid param: {:08b}",
                request_type,
            );
            Ok(res?)
        }

        fn usb_reset(&self) -> Result<Self::Reset, Self::Error> {
            Ok(self.usb.borrow_mut().reset()?)
        }

        fn memory_layout(&self) -> &dfu_core::memory_layout::mem {
            &self.memory_layout
        }
    }

    impl<'usb> DfuLibusb<'usb> {
        pub fn open(
            context: &'usb libusb::Context,
            vid: u16,
            pid: u16,
        ) -> Result<Dfu<'usb>, Error> {
            let usb = context
                .open_device_with_vid_pid(vid, pid)
                .ok_or(Error::CouldNotOpenDevice)?;
            let (index, address, transfer_size, memory_layout) = Self::query_device(&usb)?;
            let buffer = bytes::BytesMut::with_capacity(transfer_size as usize);
            let timeout = std::time::Duration::from_secs(3);
            let io = DfuLibusb {
                usb: RefCell::new(usb),
                memory_layout,
                timeout,
                index,
            };

            Ok(dfu_core::sync::DfuSync::new(io, address, transfer_size))
        }

        fn query_device(
            usb: &libusb::DeviceHandle<'usb>,
        ) -> Result<(u16, u32, u32, dfu_core::memory_layout::MemoryLayout), Error> {
            use std::convert::TryFrom;

            // TODO
            Ok((
                0,
                0x08000000,
                2048,
                dfu_core::memory_layout::MemoryLayout::try_from("04*032Kg,01*128Kg,07*256Kg")
                    .map_err(|err| Error::MemoryLayout(err.to_string()))?,
            ))
        }
    }

    #[test]
    #[should_panic]
    fn ensure_io_can_be_made_into_an_object() {
        let boxed: Box<
            dyn dfu_core::DfuIo<Read = (), Write = (), Reset = (), Error = dfu_core::Error>,
        > = panic!();
    }

    #[test]
    #[allow(unused_variables)]
    pub fn test() {
        let context = libusb::Context::new().unwrap();
        let file = std::fs::File::open("/home/cecile/Downloads/SV-F777-v0.0.8.16.bin").unwrap();
        //let file = std::fs::File::open("/home/cecile/repos/dfuflash/testfile").unwrap();

        use std::cell::RefCell;
        use std::rc::Rc;
        let file_size = 840516;
        let progress = Rc::new(RefCell::new(0));

        let mut device: Dfu = DfuLibusb::open(&context, 0x0483, 0xdf11).unwrap();
        device = device.with_progress(move |count| {
            *progress.borrow_mut() += count;
            println!("{}/{}", *progress.borrow(), file_size);
        });

        if let Err(err) = device.download(file, file_size) {
            panic!("{}", err);
        }
    }
}
