use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("The data is too short (got: {0}, expected: 9)")]
    DataTooShort(usize),
}

#[derive(Debug, Copy, Clone)]
pub struct FunctionalDescriptor {
    pub can_download: bool,
    pub can_upload: bool,
    pub manifestation_tolerant: bool,
    pub will_detach: bool,
    pub detach_timeout: u16,
    pub transfer_size: u16,
    pub dfu_version: (u8, u8),
}

impl FunctionalDescriptor {
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
