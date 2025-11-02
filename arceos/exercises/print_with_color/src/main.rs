#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]

#[cfg(feature = "axstd")]
use axstd::println;

#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    // ANSI color code: \u{1B}[32m for green
    println!("\u{1B}[32m[WithColor]: Hello, Arceos!\u{1B}[m");
}
