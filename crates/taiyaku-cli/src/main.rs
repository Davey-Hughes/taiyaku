//! Command-line aligner for taiyaku.
//!
//! Nothing is implemented yet. The intended surface, from the design doc:
//!
//!   taiyaku hash <archive>...     fingerprint pages into the content-addressed cache
//!   taiyaku align <edition-a> <edition-b>   solve the correspondence, write taiyaku.json
//!   taiyaku verify --html         contact sheet of paired thumbnails for eyeballing
//!
//! An argument parser gets added with the first real command; adding one now
//! would only be guessing at flags.

fn main() {
    println!("taiyaku {}", env!("CARGO_PKG_VERSION"));
    println!("No commands implemented yet. See docs/ for the design.");
}
