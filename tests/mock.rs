use std::{cell::RefCell, convert::TryFrom};

use bytes::{Buf, BufMut};
use dfu_core::{
    functional_descriptor::FunctionalDescriptor, memory_layout::MemoryLayout, DfuProtocol, State,
    Status,
};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use thiserror::Error;

/// Non-camel case naming to match the names in the DFU 1.1 spec
#[allow(non_camel_case_types)]
#[derive(Clone, Debug, Eq, PartialEq, FromPrimitive)]
enum Request {
    DFU_DETACH = 0,
    DFU_DNLOAD = 1,
    DFU_UPLOAD = 2,
    DFU_GETSTATUS = 3,
    DFU_CLRSTATUS = 4,
    DFU_GETSTATE = 5,
    DFU_ABORT = 6,
}

// All requests for DFU are for request type class and recipient interface
// dfu-core does not set the direction so read/write aren't distinguished
const REQUEST_TYPE: u8 = 0b00100001;

#[derive(Debug, Clone, Default)]
pub struct MockIOBuilder {
    manifestation_tolerant: bool,
    will_detach: bool,
    // STM dfu extensions (dfuse)
    dfuse: bool,
}

impl MockIOBuilder {
    pub fn manifestation_tolerant(mut self, tolerant: bool) -> Self {
        self.manifestation_tolerant = tolerant;
        self
    }

    pub fn will_detach(mut self, will_detach: bool) -> Self {
        self.will_detach = will_detach;
        self
    }

    pub fn dfuse(mut self, dfuse: bool) -> Self {
        self.dfuse = dfuse;
        self
    }

    pub fn build(self) -> MockIO {
        let (dfu_version, protocol) = if !self.dfuse {
            ((0x1, 0x10), DfuProtocol::Dfu)
        } else {
            (
                (0x1, 0x1a),
                DfuProtocol::Dfuse {
                    address: 0x0,
                    // 1024 pages of 4 bytes;
                    memory_layout: MemoryLayout::try_from("1024*4 g").unwrap(),
                },
            )
        };

        let functional_descriptor = FunctionalDescriptor {
            can_download: true,
            can_upload: false,
            manifestation_tolerant: self.manifestation_tolerant,
            will_detach: self.will_detach,
            detach_timeout: 64,
            transfer_size: 6,
            dfu_version,
        };

        let inner = RefCell::new(MockIOInner {
            state: State::DfuIdle,
            status: Status::Ok,
            download: Vec::new(),
            writes: 0,
            erased: Vec::new(),
            busy: 0,
            was_reset: false,
        });

        MockIO {
            functional_descriptor,
            protocol,
            inner,
        }
    }
}

struct MockIOInner {
    state: State,
    status: Status,
    download: Vec<u8>,
    writes: u16,
    erased: Vec<(u32, u32)>,
    busy: u16,
    was_reset: bool,
}

pub struct MockIO {
    functional_descriptor: FunctionalDescriptor,
    protocol: DfuProtocol<MemoryLayout>,
    inner: RefCell<MockIOInner>,
}

impl MockIO {
    fn state(&self) -> State {
        self.inner.borrow().state
    }

    fn update_state(&self, state: State) {
        self.inner.borrow_mut().state = state;
    }

    pub fn status(&self) -> Status {
        self.inner.borrow().status
    }

    fn status_request(&self, buffer: &mut [u8], state: State) -> Result<usize, Error> {
        buffer[0] = self.status().into(); // status ok
        (&mut buffer[1..]).put_uint_le(10, 3); // idle time
        buffer[4] = state.into();
        buffer[5] = 0; // iString descriptor
        Ok(6)
    }

    fn download_request_dfu(&self, blocknum: u16, buffer: &[u8]) {
        let mut inner = self.inner.borrow_mut();
        assert_eq!(inner.writes, blocknum);
        inner.busy = inner.writes * 2;
        inner.writes += 1;
        inner.download.extend_from_slice(buffer);
    }

    fn check_erasures(&self, buffer: &[u8]) {
        let inner = self.inner.borrow_mut();
        let mut start = inner.download.len() as u32;
        let end = start + buffer.len() as u32;
        'l: loop {
            for e in &inner.erased {
                if e.0 <= start && e.0 + e.1 > start {
                    start = e.0 + e.1;
                    if start >= end {
                        break 'l;
                    } else {
                        continue 'l;
                    }
                }
            }

            panic!("Unerased section: {} - {}", start, end);
        }
    }

    fn download_request_dfuse(&self, blocknum: u16, buffer: &[u8]) {
        match blocknum {
            0 => match buffer[0] {
                0x21 => {
                    // set address
                    let addr = buffer[1..].as_ref().get_u32_le();
                    assert_eq!(addr, self.inner.borrow().download.len() as u32);
                }
                0x41 => {
                    // erase page
                    let addr = buffer[1..].as_ref().get_u32_le();
                    let mut inner = self.inner.borrow_mut();
                    inner.erased.push((addr, 4));
                }
                cmd => todo!("Command not supported: {}", cmd),
            },
            1 => panic!("STM reserved block"),
            _ => {
                self.check_erasures(buffer);
                self.download_request_dfu(blocknum - 2, buffer)
            }
        }
    }

    fn download_request(&self, blocknum: u16, buffer: &[u8]) {
        match self.protocol {
            DfuProtocol::Dfu => self.download_request_dfu(blocknum, buffer),
            DfuProtocol::Dfuse { .. } => self.download_request_dfuse(blocknum, buffer),
        }
    }

    pub fn downloaded(self) -> Vec<u8> {
        self.inner.into_inner().download
    }

    pub fn completed(&self) -> bool {
        matches!(self.state(), State::DfuManifestWaitReset | State::DfuIdle)
    }

    pub fn was_reset(&self) -> bool {
        self.inner.borrow().was_reset
    }

    pub fn busy_cycles(&self, cycles: u16) {
        self.inner.borrow_mut().busy = cycles;
    }

    pub fn still_busy(&self) -> bool {
        let mut inner = self.inner.borrow_mut();
        if inner.busy > 0 {
            inner.busy -= 1;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Dfu(#[from] dfu_core::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

impl dfu_core::DfuIo for MockIO {
    type Read = usize;
    type Write = usize;
    type Reset = ();
    type Error = Error;
    type MemoryLayout = MemoryLayout;

    fn read_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &mut [u8],
    ) -> Result<Self::Read, Self::Error> {
        assert_eq!(request_type, REQUEST_TYPE);
        let request = Request::from_u8(request).expect("Unknown request");
        match (request, self.state()) {
            (Request::DFU_GETSTATUS, State::DfuDnloadSync) => {
                if self.still_busy() {
                    self.status_request(buffer, State::DfuDnbusy)
                } else {
                    self.update_state(State::DfuDnloadIdle);
                    self.status_request(buffer, State::DfuDnloadIdle)
                }
            }
            (Request::DFU_GETSTATUS, State::DfuManifestSync) => {
                if !self.functional_descriptor.manifestation_tolerant {
                    self.update_state(State::DfuManifestWaitReset);
                    self.status_request(buffer, State::DfuManifest)
                } else if self.still_busy() {
                    self.status_request(buffer, State::DfuManifest)
                } else {
                    self.update_state(State::DfuIdle);
                    self.status_request(buffer, State::DfuIdle)
                }
            }
            (Request::DFU_GETSTATUS, _) => {
                assert_eq!(value, 0);
                self.status_request(buffer, self.state())
            }
            (request, state) => panic!(
                "Unexpected read request: {:?} in state {:?}",
                request, state
            ),
        }
    }

    fn write_control(
        &self,
        request_type: u8,
        request: u8,
        value: u16,
        buffer: &[u8],
    ) -> Result<Self::Write, Self::Error> {
        assert_eq!(request_type, REQUEST_TYPE);
        let request = Request::from_u8(request).expect("Unknown request");
        match (request, self.state()) {
            (Request::DFU_DNLOAD, State::DfuIdle | State::DfuDnloadIdle) => {
                if buffer.is_empty() {
                    assert_eq!(self.state(), State::DfuDnloadIdle);
                    self.busy_cycles(3);
                    self.update_state(State::DfuManifestSync);
                } else {
                    self.update_state(State::DfuDnloadSync);
                    self.download_request(value, buffer);
                }
                Ok(buffer.len())
            }
            (request, state) => panic!(
                "Unexpected write request: {:?} in state {:?}",
                request, state
            ),
        }
    }

    fn usb_reset(&self) -> Result<Self::Reset, Self::Error> {
        self.inner.borrow_mut().was_reset = true;
        assert_eq!(
            self.state(),
            State::DfuManifestWaitReset,
            "Wrong state for reset: {:?}",
            self.state()
        );
        assert!(!self.functional_descriptor.will_detach, "Unexpected Reset");
        Ok(())
    }

    fn functional_descriptor(&self) -> &dfu_core::functional_descriptor::FunctionalDescriptor {
        &self.functional_descriptor
    }

    fn protocol(&self) -> &dfu_core::DfuProtocol<Self::MemoryLayout> {
        &self.protocol
    }
}
