//! Rust bindings for C libraries.
//!
//! See [README](https://docs.rs/crate/clib/0.1.0/source/README.md) for more.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
