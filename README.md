![Rust](https://github.com/dfu-rs/dfu-core/workflows/main/badge.svg)
[![Latest Version](https://img.shields.io/crates/v/dfu-core.svg)](https://crates.io/crates/dfu-core)
![License](https://img.shields.io/crates/l/dfu-core)
[![Docs.rs](https://docs.rs/dfu-core/badge.svg)](https://docs.rs/dfu-core)
[![LOC](https://tokei.rs/b1/github/dfu-rs/dfu-core)](https://github.com/dfu-rs/dfu-core)
[![Dependency Status](https://deps.rs/repo/github/dfu-rs/dfu-core/status.svg)](https://deps.rs/repo/github/dfu-rs/dfu-core)

dfu-core
========

Sans IO core library (traits and tools) for DFU.

Features
--------

 -  [x] `no_std` compatible
 -  [x] async and sync compatible
 -  [x] write a firmware into a device (DFU download)
 -  [ ] read a firmware from a device (DFU upload)
 -  [x] minimal dependencies
 -  [x] uses a state machine to ensure the implementations are correctly done

Traits & Structs
----------------

 -  `trait DfuIo`: a trait that can be made into an object that provides the IO
    to this library logic.
 -  `struct DfuSansIo`: a struct that allows the developer to do the DFU logic
    using a state machine (can be async or sync).
 -  `struct DfuSync`: a basic sync implementation that uses a `DfuIo` provided
    in argument during runtime.
 -  `type MemoryPage` and `type mem`: primitives representing the memory layout
    of the device (like `char` and `str`).
 -  `struct MemoryLayout`: (requires features `std`) an allocated
    representation of the memory layout (like `String`) that can parse a
    memory layout from a string.
 -  `FunctionalDescriptor`: can read the extra bytes of a USB functional
    descriptor to provide information for the DFU logic.

DFU Documentation
-----------------

This crate has been made based on the following specifications:

DFU 1.1 (Aug 5 2004): https://www.usb.org/sites/default/files/DFU_1.1.pdf

License
-------

MIT OR Apache-2.0
