use dfu_core::DfuIo;
use mock::MockIO;

mod mock;

fn setup() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .parse_default_env()
        .try_init();
}

fn test_simple_download(mock: MockIO) {
    let firmware = b"thisisnotafirmwareorisit";
    let mut dfu = dfu_core::sync::DfuSync::new(mock);

    dfu.download_from_slice(firmware).unwrap();
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
