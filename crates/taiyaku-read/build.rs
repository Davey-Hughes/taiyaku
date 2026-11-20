//! Sets the `rar` cfg when a RAR backend is compiled in, so the `rar` module
//! and what reads RAR through it build only then; and, with the libarchive
//! backend, hands the crate the libarchive version its reader signature names.

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
    #[cfg(feature = "rar-libarchive")]
    libarchive_version();
}

/// Exports the version of the libarchive pkg-config finds as
/// `TAIYAKU_LIBARCHIVE_VERSION`, read where it was built rather than where it
/// runs (ruling P2-R5). compress-tools' own build script links the library,
/// so this probe emits no link lines of its own.
#[cfg(feature = "rar-libarchive")]
fn libarchive_version() {
    let library = pkg_config::Config::new()
        .cargo_metadata(false)
        .probe("libarchive")
        .unwrap_or_else(|e| {
            panic!(
                "the rar-libarchive feature needs libarchive's development files, found \
                 through pkg-config: install libarchive-dev and pkg-config (Debian, Ubuntu) \
                 or libarchive and pkgconf (Arch). pkg-config said: {e}"
            )
        });
    println!(
        "cargo::rustc-env=TAIYAKU_LIBARCHIVE_VERSION={}",
        library.version
    );
}
