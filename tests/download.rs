use dfu_core::DfuIo;
use mock::MockIO;

mod mock;

struct TestCursor<'a> {
    data: &'a [u8],
    offset: usize,
    reads: usize,
}

impl<'a> TestCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            offset: 0,
            reads: 0,
        }
    }
}

impl std::io::Read for TestCursor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.data.len() - self.offset;

        // Do 3 full reads and then 4 partial reads
        let tocopy = if self.reads % 7 < 3 {
            buf.len().min(remaining)
        } else {
            (buf.len() / 2 + 1).min(remaining)
        };

        let dst = &mut buf[0..tocopy];
        let src = &self.data[self.offset..self.offset + tocopy];
        dst.copy_from_slice(src);

        self.offset += tocopy;
        self.reads += 1;

        Ok(tocopy)
    }
}

fn setup() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .parse_default_env()
        .try_init();
}

fn test_simple_download(mock: MockIO) {
    let size = mock.size();
    let mut firmware = Vec::with_capacity(size as usize);
    for i in 0..size {
        firmware.push(i as u8);
    }

    let cursor = TestCursor::new(&firmware);
    let mut dfu = dfu_core::sync::DfuSync::new(mock);

    dfu.download(cursor, firmware.len() as u32).unwrap();
    let mock = dfu.into_inner();

    let descriptor = mock.functional_descriptor();

    assert_eq!(
        mock.was_reset(),
        !descriptor.manifestation_tolerant && !descriptor.will_detach
    );
    assert!(mock.completed());
    assert_eq!(firmware, mock.downloaded().as_slice());
}

#[test]
fn no_will_detach_and_no_manifestation_toleration() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(false)
        .manifestation_tolerant(false)
        .build();
    test_simple_download(mock);
}

#[test]
fn will_detach_and_no_manifestation_toleration() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .manifestation_tolerant(false)
        .will_detach(true)
        .build();
    test_simple_download(mock);
}

#[test]
fn no_will_detach_and_manifestation_toleration() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(false)
        .manifestation_tolerant(true)
        .build();
    test_simple_download(mock);
}

#[test]
fn will_detach_and_manifestation_toleration() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(true)
        .manifestation_tolerant(true)
        .build();
    test_simple_download(mock);
}

#[test]
fn no_will_detach_and_no_manifestation_toleration_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(false)
        .manifestation_tolerant(false)
        .dfuse(true)
        .build();
    test_simple_download(mock);
}

#[test]
fn will_detach_and_no_manifestation_toleration_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .manifestation_tolerant(false)
        .will_detach(true)
        .dfuse(true)
        .build();
    test_simple_download(mock);
}

#[test]
fn no_will_detach_and_manifestation_toleration_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(false)
        .manifestation_tolerant(true)
        .dfuse(true)
        .build();
    test_simple_download(mock);
}

#[test]
fn will_detach_and_manifestation_toleration_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(true)
        .manifestation_tolerant(true)
        .dfuse(true)
        .build();
    test_simple_download(mock);
}
