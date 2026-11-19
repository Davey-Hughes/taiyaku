//! Sets the `rar` cfg when a RAR backend is compiled in, so the `rar` module
//! and what reads RAR through it build only then.

/// Each RAR backend's feature as cargo hands it to a build script, in the
/// order `rar::BACKENDS` lists the backends: the first compiled one is the
/// one the library uses.
const BACKENDS: &[&str] = &["CARGO_FEATURE_RAR_LIBARCHIVE"];

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(rar)");
    if BACKENDS
        .iter()
        .any(|feature| std::env::var_os(feature).is_some())
    {
        println!("cargo::rustc-cfg=rar");
    }
}
