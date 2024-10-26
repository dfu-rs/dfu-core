use dfu_core::asynchronous::DfuAsyncIo;
use futures::AsyncRead;
use futures_test::test;
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

impl AsyncRead for TestCursor<'_> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let me = self.get_mut();
        let remaining = me.data.len() - me.offset;

        // Do 3 full reads and then 4 partial reads
        let tocopy = if me.reads % 7 < 3 {
            buf.len().min(remaining)
        } else {
            (buf.len() / 2 + 1).min(remaining)
        };

        let dst = &mut buf[0..tocopy];
        let src = &me.data[me.offset..me.offset + tocopy];
        dst.copy_from_slice(src);

        me.offset += tocopy;
        me.reads += 1;

        std::task::Poll::Ready(Ok(tocopy))
    }
}

fn setup() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .parse_default_env()
        .try_init();
}

async fn test_simple_download(mock: MockIO) {
    let size = mock.size();
    let address = mock.address();
    let mut firmware = Vec::with_capacity(size as usize);
    for i in 0..size {
        firmware.push(i as u8);
    }

    let cursor = TestCursor::new(&firmware);
    let mut dfu = dfu_core::asynchronous::DfuASync::new(mock);

    if let Some(address) = address {
        dfu.override_address(address);
    }

    dfu.download(cursor, firmware.len() as u32).await.unwrap();
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
async fn no_will_detach_and_no_manifestation_toleration() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(false)
        .manifestation_tolerant(false)
        .build();
    test_simple_download(mock).await;
}

#[test]
async fn will_detach_and_no_manifestation_toleration() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .manifestation_tolerant(false)
        .will_detach(true)
        .build();
    test_simple_download(mock).await;
}

#[test]
async fn no_will_detach_and_manifestation_toleration() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(false)
        .manifestation_tolerant(true)
        .build();
    test_simple_download(mock).await;
}

#[test]
async fn will_detach_and_manifestation_toleration() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(true)
        .manifestation_tolerant(true)
        .build();
    test_simple_download(mock).await;
}

#[test]
async fn no_will_detach_and_no_manifestation_toleration_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(false)
        .manifestation_tolerant(false)
        .dfuse(true)
        .build();
    test_simple_download(mock).await;
}

#[test]
async fn will_detach_and_no_manifestation_toleration_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .manifestation_tolerant(false)
        .will_detach(true)
        .dfuse(true)
        .build();
    test_simple_download(mock).await;
}

#[test]
async fn no_will_detach_and_manifestation_toleration_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(false)
        .manifestation_tolerant(true)
        .dfuse(true)
        .build();
    test_simple_download(mock).await;
}

#[test]
async fn will_detach_and_manifestation_toleration_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .will_detach(true)
        .manifestation_tolerant(true)
        .dfuse(true)
        .build();
    test_simple_download(mock).await;
}

#[test]
async fn override_address_dfuse() {
    setup();
    let mock = mock::MockIOBuilder::default()
        .address(0x08004000)
        .dfuse(true)
        .build();
    test_simple_download(mock).await;
}
