Changelog
=========

## v0.6.0

- Leave asynchronous sleep implementation up to the user instead of using blocking `std::thread::sleep` 

## v0.5.0

- Add tests (#18)
- Support devices that are manifestation tolerant (#19)
- Support both dfu 1.1 and dfuse protocols (#20)

## v0.4.2

- Fix clearing status not working (#16)

## v0.4.1

- Improve logging (#12)
- Use mutable reference instead of move (#11)
- Skip clear status conditionally (#13)
- Add method download_all() (#14)

## v0.3.0

- Change progress function to FnMut (#10)
- Add method override_address() to change the address (#9)

## v0.2.1

- Add write from slice function (#6)
- Add all features to doc for rustdoc (#8)

## v0.2.0

- Use String on error type for memory layout (#5)
- Fix for memory layout for STM32/DeFuse extensions (#3)
