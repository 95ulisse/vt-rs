# vt-rs

[![Latest version](https://img.shields.io/crates/v/vt.svg)](https://crates.io/crates/vt)
[![Documentation](https://docs.rs/vt/badge.svg)](https://docs.rs/vt)
[![License](https://img.shields.io/crates/l/vt)](LICENSE)

Rust bindings for the Linux virtual terminal APIs.

Documentation: [https://docs.rs/vt](https://docs.rs/vt)

## Example

```rust
use std::io::Write;
use vt::Console;

// First of all, get a handle to the console
let console = Console::open().unwrap();

// Allocate a new virtual terminal
let mut vt = console.new_vt().unwrap();

// Write something to it.
// A `Vt` structure implements both `std::io::Read` and `std::io::Write`.
writeln!(vt, "Hello world!");

// Switch to the newly allocated terminal
vt.switch().unwrap();
```

## License

`vt-rs` is released under the MIT license. For more information, see [LICENSE](LICENSE).
