# blocking-permit

[![Travis CI Build](https://travis-ci.org/dekellum/blocking-permit.svg?branch=master)](https://travis-ci.org/dekellum/blocking-permit)
[![MSRV](https://img.shields.io/badge/rustc-%E2%89%A5%201.39-orange.svg)](https://github.com/rust-lang/rust/blob/master/RELEASES.md)

This crate provides:

* A specialized, custom thread pool, `DispatchPool`, for offloading
  blocking or otherwise long running operations from a main or reactor
  threads.

* A `BlockingPermit` for limiting the number of concurrent blocking operations
  via a [`Semaphore`] type.

## Minimum supported rust version

MSRV := 1.39.0

The crate will fail fast on any lower rustc (via a build.rs version
check) and is also CI tested on this version.

## License

This project is dual licensed under either of following:

* The Apache License, version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)
  or http://www.apache.org/licenses/LICENSE-2.0)

* The MIT License ([LICENSE-MIT](LICENSE-MIT)
  or http://opensource.org/licenses/MIT)

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in blocking-permit by you, as defined by the Apache License, shall be
dual licensed as above, without any additional terms or conditions.
