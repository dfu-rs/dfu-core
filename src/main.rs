#![allow(unused_variables)]

pub mod dfu_core {
    include!("./lib.rs");
}

use std::cell::RefCell;
use thiserror::Error;

pub type Dfu<'usb> = dfu_core::sync::DfuSync<DfuLibusb<'usb>, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Could not find device or an error occurred.")]
    CouldNotOpenDevice,
    #[error(transparent)]
    Dfu(#[from] dfu_core::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Could not parse memory layout: {0}")]
    MemoryLayout(String),
    #[error("libusb: {0}")]
    LibUsb(#[from] libusb::Error),
    #[error("The device has no languages.")]
    MissingLanguage,
    #[error("Could not find interface.")]
    InvalidInterface,
    #[error("Could not find alt interface.")]
    InvalidAlt,
    #[error("Could not parse interface string.")]
    InvalidInterfaceString,
    #[error("Could not parse address.")]
    InvalidAddress,
}

pub struct DfuLibusb<'usb> {
    usb: RefCell<libusb::DeviceHandle<'usb>>,
    memory_layout: dfu_core::memory_layout::MemoryLayout,
    timeout: std::time::Duration,
    index: u16,
    lang: libusb::Language,
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
        let res = self.usb.borrow().read_control(
            request_type,
            request,
            value,
            self.index,
            buffer,
            self.timeout,
        );
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
        iface: u8,
        alt: u8,
    ) -> Result<Dfu<'usb>, Error> {
        let (device, mut handle) = Self::open_device(context, vid, pid)?;
        handle.set_alternate_setting(iface, alt);
        let (index, lang, timeout, address, transfer_size, memory_layout) =
            Self::query_device(device, &handle, iface, alt)?;
        let buffer = bytes::BytesMut::with_capacity(transfer_size as usize);
        let io = DfuLibusb {
            usb: RefCell::new(handle),
            memory_layout,
            timeout,
            index,
            lang,
        };

        Ok(dfu_core::sync::DfuSync::new(io, address, transfer_size))
    }

    fn open_device(
        context: &'usb libusb::Context,
        vid: u16,
        pid: u16,
    ) -> Result<(libusb::Device<'usb>, libusb::DeviceHandle<'usb>), Error> {
        for device in context.devices()?.iter() {
            let device_desc = match device.device_descriptor() {
                Ok(x) => x,
                Err(_) => continue,
            };

            if device_desc.vendor_id() == vid && device_desc.product_id() == pid {
                let handle = device.open()?;
                return Ok((device, handle));
            }
        }

        Err(Error::CouldNotOpenDevice)
    }

    fn query_device(
        device: libusb::Device<'usb>,
        handle: &libusb::DeviceHandle<'usb>,
        iface: u8,
        alt: u8,
    ) -> Result<
        (
            u16,
            libusb::Language,
            std::time::Duration,
            u32,
            u32,
            dfu_core::memory_layout::MemoryLayout,
        ),
        Error,
    > {
        use std::convert::TryFrom;

        let index = 0;
        let timeout = std::time::Duration::from_secs(3);

        let languages = handle.read_languages(timeout)?;
        let lang = languages.get(0).ok_or(Error::MissingLanguage)?;
        let config_descriptor = device.config_descriptor(0)?;

        let interface = config_descriptor
            .interfaces()
            .find(|x| x.number() == iface)
            .ok_or(Error::InvalidInterface)?;
        let iface_desc = interface
            .descriptors()
            .find(|x| x.setting_number() == alt)
            .ok_or(Error::InvalidAlt)?;
        let interface_string = handle.read_interface_string(*lang, &iface_desc, timeout)?;

        let (rest, memory_layout) = interface_string
            .rsplit_once('/')
            .ok_or(Error::InvalidInterfaceString)?;
        let memory_layout = dfu_core::memory_layout::MemoryLayout::try_from(memory_layout)
            .map_err(|err| Error::MemoryLayout(err.to_string()))?;
        let (rest, address) = rest.rsplit_once('/').ok_or(Error::InvalidInterfaceString)?;
        let address = address
            .strip_prefix("0x")
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .ok_or(Error::InvalidAddress)?;

        //todo!("{}", handle.read_string_descriptor(*lang, 0, timeout)?);

        /*
        let mut buffer = [0x00; 9];
        assert_eq!(((0x21 << 8) | 0), 0x2100);
        #[allow(arithmetic_overflow)]
        let dfu_func = handle.read_control(
            0x80,
            libusb_sys::LIBUSB_REQUEST_GET_DESCRIPTOR,
            (0x21 << 8) | 0,
            0,
            &mut buffer,
            timeout,
        )?;
        todo!("{:?}", dfu_func);
        */

        // TODO
        Ok((index, *lang, timeout, address, 2048, memory_layout))
    }
}

fn main() {
    use std::convert::TryFrom;
    use std::io;
    use std::io::Seek;

    let file_path = std::env::args()
        .skip(1)
        .next()
        .expect("missing path to firmware");

    let context = libusb::Context::new().unwrap();
    let mut file = std::fs::File::open(file_path).unwrap();
    //let file = std::fs::File::open("/home/cecile/repos/dfuflash/testfile").unwrap();

    use std::cell::RefCell;
    use std::rc::Rc;
    let file_size = u32::try_from(file.seek(io::SeekFrom::End(0)).unwrap()).unwrap();
    file.seek(io::SeekFrom::Start(0)).unwrap();
    let progress = Rc::new(RefCell::new(0));

    let mut device: Dfu = DfuLibusb::open(&context, 0x0483, 0xdf11, 0, 0).unwrap();
    device = device.with_progress(move |count| {
        *progress.borrow_mut() += count;
        println!("{}/{}", *progress.borrow(), file_size);
    });

    if let Err(err) = device.download(file, file_size) {
        panic!("{}", err);
    }
}
