use displaydoc::Display;
#[cfg(any(feature = "std", test))]
use thiserror::Error;

/// Error when reading the functional descriptor.
#[derive(Debug, Display)]
#[cfg_attr(any(feature = "std", test), derive(Error))]
pub enum Error {
    /// The data is too short (got: {0}, expected: 9).
    DataTooShort(usize),
}

/// Functional descriptor.
#[derive(Debug, Copy, Clone)]
pub struct FunctionalDescriptor {
    /// bitCanDnload.
    ///
    /// Bit 0: download capable.
    pub can_download: bool,
    /// bitCanUpload.
    ///
    /// Bit 1: upload capable.
    pub can_upload: bool,
    /// bitManifestationTolerant.
    ///
    /// Bit 2: device is able to communicate via USB after Manifestation phase.
    pub manifestation_tolerant: bool,
    /// bitWillDetach.
    ///
    /// Bit 3: device will perform a bus detach-attach sequence when it receives a DFU_DETACH
    /// request. The host must not issue a USB Reset.
    pub will_detach: bool,
    /// wDetachTimeOut.
    ///
    /// Time, in milliseconds, that the device will wait after receipt of the DFU_DETACH request.
    /// If this time elapses without a USB reset, then the device will terminate the
    /// Reconfiguration phase and revert back to normal operation. This represents the maximum
    /// time that the device can wait (depending on its timers, etc.). The host may specify a
    /// shorter timeout in the DFU_DETACH request.
    pub detach_timeout: u16,
    /// wTransferSize.
    ///
    /// Maximum number of bytes that the device can accept per control-write transaction.
    pub transfer_size: u16,
    /// bcdDFUVersion.
    ///
    /// Numeric expression identifying the version of the DFU Specification release.
    pub dfu_version: (u8, u8),
}

impl FunctionalDescriptor {
    /// Read functional descriptor from a slice of bytes.
    pub fn from_bytes(mut bytes: &[u8]) -> Option<Result<Self, Error>> {
        use bytes::Buf;

        let len = bytes.len();
        if len < 2 {
            return None;
        }

        bytes.advance(1);

        let descriptor_type = bytes.get_u8();

        if descriptor_type != 0x21 {
            return None;
        }

        if len < 9 {
            return Some(Err(Error::DataTooShort(len)));
        }

        let attributes = bytes.get_u8();
        let can_download = attributes & (1 << 0) > 0;
        let can_upload = attributes & (1 << 1) > 0;
        let manifestation_tolerant = attributes & (1 << 2) > 0;
        let will_detach = attributes & (1 << 3) > 0;

        let detach_timeout = bytes.get_u16_le();
        let transfer_size = bytes.get_u16_le();
        let minor = bytes.get_u8();
        let major = bytes.get_u8();
        let dfu_version = (major, minor);

        Some(Ok(Self {
            can_download,
            can_upload,
            manifestation_tolerant,
            will_detach,
            detach_timeout,
            transfer_size,
            dfu_version,
        }))
    }
}
