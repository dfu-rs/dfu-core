# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.1] - 2026-06-01

### Added

- Async download progress callback (#33)

## [0.11.0] - 2026-05-05

### Changed

- Download functions now take ownership of the DFU handle (#32)

## [0.10.0] - 2025-12-10

### Changed

- `usb_reset` on `DfuIo` / `DfuAsyncIo` now accepts `&mut self` (#31)

## [0.9.2] - 2025-12-09

### Fixed

- Update CHANGELOG.md

## [0.9.0] - 2025-11-12

### Fixed

- Allow `block_num` to wrap per DFU spec 6.1.1 (#30)

## [0.8.0] - 2025-10-24

### Fixed

- `std::thread::sleep` should not be used in `async` context (#28)

## [0.7.0] - 2024-11-09

### Added

- Async support (#27)

## [0.6.0] - 2023-05-10

### Added

- Ability to override download address (#25)

## [0.5.0] - 2023-01-26

### Added

- Tests (#18)
- Support for devices that are manifestation tolerant (#19)
- Support for both DFU 1.1 and DfuSe protocols (#20)

## [0.4.2] - 2022-07-31

### Fixed

- Clearing status not working (#16)

## [0.4.1] - 2022-07-31

### Added

- Method `download_all()` (#14)

### Changed

- Use mutable reference instead of move (#11)
- Skip clear status conditionally (#13)

### Fixed

- Improve logging (#12)

## [0.3.0] - 2022-04-20

### Added

- Method `override_address()` to change the address (#9)

### Changed

- Progress function changed to `FnMut` (#10)

## [0.2.1] - 2022-02-16

### Added

- Write from slice function (#6)
- All features to doc for rustdoc (#8)

## [0.2.0] - 2022-01-29

### Changed

- Use `String` on error type for memory layout (#5)

### Fixed

- Memory layout parsing for STM32/DfuSe extensions (#3)

[Unreleased]: https://github.com/dfu-rs/dfu-core/compare/v0.11.1...HEAD
[0.11.1]: https://github.com/dfu-rs/dfu-core/compare/v0.11.0...v0.11.1
[0.11.0]: https://github.com/dfu-rs/dfu-core/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/dfu-rs/dfu-core/compare/v0.9.2...v0.10.0
[0.9.2]: https://github.com/dfu-rs/dfu-core/compare/v0.9.1...v0.9.2
[0.9.0]: https://github.com/dfu-rs/dfu-core/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/dfu-rs/dfu-core/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/dfu-rs/dfu-core/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/dfu-rs/dfu-core/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/dfu-rs/dfu-core/compare/v0.4.2...v0.5.0
[0.4.2]: https://github.com/dfu-rs/dfu-core/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/dfu-rs/dfu-core/compare/v0.3.0...v0.4.1
[0.3.0]: https://github.com/dfu-rs/dfu-core/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/dfu-rs/dfu-core/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/dfu-rs/dfu-core/releases/tag/v0.2.0
