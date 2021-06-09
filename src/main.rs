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
    // TODO: it's kinda annoying to have a lifetime in the error
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
    #[error("Could not parse functional descriptor: {0}")]
    FunctionalDescriptor(#[from] dfu_core::functional_descriptor::Error),
    #[error("No DFU capable device found")]
    NoDfuCapableDeviceFound,
}

pub struct DfuLibusb<'usb> {
    usb: RefCell<libusb::DeviceHandle<'usb>>,
    memory_layout: dfu_core::memory_layout::MemoryLayout,
    timeout: std::time::Duration,
    iface: u16,
    functional_descriptor: dfu_core::functional_descriptor::FunctionalDescriptor,
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
        // TODO: do or do not? there is no try
        let request_type = request_type | libusb_sys::LIBUSB_ENDPOINT_IN;
        let res = self.usb.borrow().read_control(
            request_type,
            request,
            value,
            self.iface,
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
            self.iface,
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

    fn functional_descriptor(&self) -> &dfu_core::functional_descriptor::FunctionalDescriptor {
        &self.functional_descriptor
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
        use std::convert::TryFrom;

        let timeout = std::time::Duration::from_secs(3);
        let (device, handle) = Self::open_device(context, vid, pid)?;
        // TODO
        //handle.set_alternate_setting(iface, alt)?;
        // TODO claim?
        let device_descriptor = device.device_descriptor()?;
        let languages = handle.read_languages(timeout)?;
        let lang = languages.iter().next().ok_or(Error::MissingLanguage)?;

        for index in 0..device_descriptor.num_configurations() {
            let config_descriptor = device.config_descriptor(index)?;

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

            if let Some(functional_descriptor) =
                Self::find_functional_descriptor(&handle, &config_descriptor, timeout)
                    .transpose()?
            {
                let io = DfuLibusb {
                    usb: RefCell::new(handle),
                    memory_layout,
                    timeout,
                    iface: iface as u16,
                    functional_descriptor,
                };

                return Ok(dfu_core::sync::DfuSync::new(io, address));
            }
        }

        Err(Error::NoDfuCapableDeviceFound)
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

    fn find_functional_descriptor(
        handle: &libusb::DeviceHandle<'usb>,
        config: &libusb::ConfigDescriptor,
        timeout: std::time::Duration,
    ) -> Option<Result<dfu_core::functional_descriptor::FunctionalDescriptor, Error>> {
        macro_rules! find_func_desc {
            ($maybe_data:expr) => {{
                if let Some(func_desc) = $maybe_data
                    .and_then(dfu_core::functional_descriptor::FunctionalDescriptor::from_bytes)
                {
                    return Some(func_desc.map_err(Into::into));
                }
            }};
        }

        find_func_desc!(config.extra());

        for if_desc in config.interfaces().map(|x| x.descriptors()).flatten() {
            find_func_desc!(if_desc.extra());
        }

        let mut buffer = [0x00; 9];
        match handle.read_control(
            libusb_sys::LIBUSB_ENDPOINT_IN,
            libusb_sys::LIBUSB_REQUEST_GET_DESCRIPTOR,
            0x2100,
            0,
            &mut buffer,
            timeout,
        ) {
            Ok(n) => find_func_desc!(Some(&buffer[..n])),
            Err(err) => return Some(Err(err.into())),
        }

        None
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
