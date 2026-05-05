![Rust](https://github.com/dfu-rs/dfu-core/workflows/main/badge.svg)
[![Latest Version](https://img.shields.io/crates/v/dfu-core.svg)](https://crates.io/crates/dfu-core)
![License](https://img.shields.io/crates/l/dfu-core)
[![Docs.rs](https://docs.rs/dfu-core/badge.svg)](https://docs.rs/dfu-core)
[![Keep a Changelog](https://img.shields.io/badge/changelog-Keep%20a%20Changelog-%23E05735)](CHANGELOG.md)
[![Dependency Status](https://deps.rs/repo/github/dfu-rs/dfu-core/status.svg)](https://deps.rs/repo/github/dfu-rs/dfu-core)

dfu-core
========

A sans-IO, `no_std`-compatible core library implementing the DFU protocol state machine.

The library drives the full DFU protocol logic — including DFU 1.1 and the STM32 DfuSe
extension — but never touches USB directly. Instead, you supply a transport by implementing
`DfuIo` (or `DfuAsyncIo`), and the library tells you exactly which control transfers to
perform and in what order. This makes it runtime-agnostic and usable in `no_std` environments.

If you are looking for a ready-to-use CLI tool or a higher-level crate that already has a
USB backend wired up, see:

- [`dfu-nusb`](https://crates.io/crates/dfu-nusb) — recommended, built on [`nusb`](https://crates.io/crates/nusb)
- [`dfu-libusb`](https://crates.io/crates/dfu-libusb) — built on [`libusb`](https://crates.io/crates/libusb)

Feature Flags
-------------

| Feature | Description |
|---------|-------------|
| *(none)* | `no_std` core: state machine, `DfuIo`, `DfuSansIo`, `FunctionalDescriptor`, `MemoryPage`, `mem` |
| `std` | Adds `MemoryLayout`, `std::error::Error` impls, `DfuProtocol::new()`, and `DfuSync` |
| `async` | Adds `DfuAsyncIo` and `DfuAsync` (implies `std`) |

API Overview
------------

**Implement one of these traits** to provide the USB transport:

- `trait DfuIo` — synchronous transport (control reads, control writes, USB reset)
- `trait DfuAsyncIo` — async transport, same operations plus a `sleep` method
  (requires feature `async`)

**Choose your level of abstraction** for the protocol logic:

- `struct DfuSync` — high-level synchronous wrapper; call `download()`,
  `download_all()`, or `download_from_slice()` and it handles the rest
  (requires feature `std`)
- `struct DfuAsync` — high-level async wrapper, mirrors `DfuSync`
  (requires feature `async`)
- `struct DfuSansIo` — low-level sans-IO state machine for `no_std` or when
  you need explicit control over each USB transaction; returns typed command
  objects (`UsbWriteControl`, `UsbReadControl`) that you execute yourself

**Supporting types:**

- `enum DfuProtocol` — selects between DFU 1.1 (`Dfu`) and STM32 DfuSe (`Dfuse`)
- `struct FunctionalDescriptor` — parsed from the extra bytes of a USB DFU
  functional descriptor; drives protocol decisions (transfer size, detach
  behaviour, manifestation tolerance)
- `type MemoryPage` and `type mem` — primitives representing the memory layout
  of the device (analogous to `char` and `str`)
- `struct MemoryLayout` — owned, heap-allocated memory layout that can parse
  the STM32 memory layout interface string (requires feature `std`)

Features
--------

- [x] `no_std` compatible
- [x] sync and async compatible
- [x] write a firmware into a device (DFU download)
- [ ] read a firmware from a device (DFU upload)
- [x] minimal dependencies
- [x] uses a state machine to ensure implementations are correct

DFU Specifications
------------------

This crate is based on the following specifications:

- [DFU 1.1 (Aug 5 2004)](https://www.usb.org/sites/default/files/DFU_1.1.pdf)
- [STM32 DfuSe extensions](https://www.st.com/content/ccc/resource/technical/document/user_manual/cc/6d/c3/43/ea/29/4b/eb/CD00135281.pdf/files/CD00135281.pdf/jcr:content/translations/en.CD00135281.pdf)

License
-------

MIT OR Apache-2.0
