//! # vt-rs
//! 
//! Rust bindings for the Linux virtual terminal APIs.
//! 
//! ```rust,no_run
//! # use std::io::Write;
//! use vt::Console;
//! 
//! // First of all, get a handle to the console
//! let console = Console::open().unwrap();
//! 
//! // Allocate a new virtual terminal
//! let mut vt = console.new_vt().unwrap();
//! 
//! // Write something to it.
//! // A `Vt` structure implements both `std::io::Read` and `std::io::Write`.
//! writeln!(vt, "Hello world!");
//! 
//! // Switch to the newly allocated terminal
//! vt.switch().unwrap();
//! ```
//! 
//! For a more complete example, see the files in the `examples` folder.

#[macro_use] extern crate bitflags;

mod ffi;
mod console;
mod vt;

pub use crate::console::*;
pub use crate::vt::*;