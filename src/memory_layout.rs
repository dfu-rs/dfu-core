#[cfg(any(feature = "std", test))]
use displaydoc::Display;
#[cfg(any(feature = "std", test))]
use std::prelude::v1::*;
#[cfg(any(feature = "std", test))]
use thiserror::Error;

/// Error while parsing a memory layout.
#[cfg(any(feature = "std", test))]
#[derive(Debug, Display, Error)]
pub enum Error {
    /// invalid page format: {0}
    InvalidPageFormat(String),
    /// could not parse page count: {0}
    ParseErrorPageCount(String),
    /// could not parse page size: {0}
    ParseErrorPageSize(String),
    /// invalid prefix: {0}
    InvalidPrefix(String),
}

/// A memory page size.
pub type MemoryPage = u32;

/// A slice of memory pages.
#[allow(non_camel_case_types)]
pub type mem = [MemoryPage];

/// Memory layout.
#[cfg(any(feature = "std", test))]
pub struct MemoryLayout(Vec<MemoryPage>);

#[cfg(any(feature = "std", test))]
impl AsRef<mem> for MemoryLayout {
    fn as_ref(&self) -> &mem {
        self.0.as_slice()
    }
}

#[cfg(any(feature = "std", test))]
impl MemoryLayout {
    /// Create a new empty instance of [`MemoryLayout`].
    pub fn new() -> Self {
        Self(Vec::new())
    }
}

#[cfg(any(feature = "std", test))]
impl Default for MemoryLayout {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(feature = "std", test))]
impl From<Vec<MemoryPage>> for MemoryLayout {
    fn from(vec: Vec<MemoryPage>) -> Self {
        Self(vec)
    }
}

#[cfg(any(feature = "std", test))]
impl core::ops::Deref for MemoryLayout {
    type Target = Vec<MemoryPage>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(any(feature = "std", test))]
impl core::ops::DerefMut for MemoryLayout {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(any(feature = "std", test))]
impl<'a> core::convert::TryFrom<&str> for MemoryLayout {
    type Error = Error;

    fn try_from(src: &str) -> Result<Self, Self::Error> {
        use core::str::FromStr;

        let mut pages = Vec::new();

        for s in src.split(',') {
            if s.len() < 8 {
                return Err(Error::InvalidPageFormat(s.into()));
            }

            let count =
                u32::from_str(&s[..2]).map_err(|_| Error::ParseErrorPageCount(s[..2].into()))?;
            let size =
                u32::from_str(&s[3..6]).map_err(|_| Error::ParseErrorPageSize(s[3..6].into()))?;
            let prefix = match &s[6..=6] {
                "K" => 1024,
                "M" => 1024 * 1024,
                " " => 1,
                other => return Err(Error::InvalidPrefix(other.into())),
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
    use core::convert::TryFrom;

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
