use std::convert::TryFrom;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error<'a> {
    #[error("invalid page format: {0}")]
    InvalidPageFormat(&'a str),
    #[error("could not parse page count: {0}")]
    ParseErrorPageCount(&'a str),
    #[error("could not parse page size: {0}")]
    ParseErrorPageSize(&'a str),
    #[error("invalid prefix: {0}")]
    InvalidPrefix(&'a str),
}

pub type MemoryPage = u32;

#[allow(non_camel_case_types)]
pub type mem = [MemoryPage];

pub struct MemoryLayout(Vec<MemoryPage>);

impl AsRef<mem> for MemoryLayout {
    fn as_ref(&self) -> &mem {
        self.0.as_slice()
    }
}

impl MemoryLayout {
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

impl std::ops::Deref for MemoryLayout {
    type Target = Vec<MemoryPage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> TryFrom<&'a str> for MemoryLayout {
    type Error = Error<'a>;

    fn try_from(src: &'a str) -> Result<Self, Self::Error> {
        let mut pages = Vec::new();

        for s in src.split(',') {
            if s.len() < 8 {
                return Err(Error::InvalidPageFormat(s));
            }

            let count = u32::from_str(&s[..2]).map_err(|_| Error::ParseErrorPageCount(&s[..2]))?;
            let size = u32::from_str(&s[3..6]).map_err(|_| Error::ParseErrorPageSize(&s[3..6]))?;
            let prefix = match &s[6..=6] {
                "K" => 1024,
                "M" => 1024 * 1024,
                " " => 1,
                other => return Err(Error::InvalidPrefix(other)),
            };

            let size = size * prefix;
            for _ in 0..count {
                pages.push(size);
            }
        }

        Ok(Self(pages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing() {
        let s = "04*032Kg,01*128Kg,07*256Kg";
        let m = MemoryLayout::try_from(s).unwrap();
        assert_eq!(
            m.as_slice(),
            &[
                32768, 32768, 32768, 32768, 131072, 262144, 262144, 262144, 262144, 262144, 262144,
                262144
            ]
        );
    }
}
