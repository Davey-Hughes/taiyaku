//! What makes a work the same work across a move, a rename or a remount.
//!
//! Content, with the path kept only as a hint. Keying on the canonical path
//! would duplicate every work in the library when a NAS mounts at `/mnt/nas` on
//! one host and `/Volumes/nas` on another, and orphan every note on it.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};
use taiyaku_core::Sidecar;

/// A work's identity: a sha256 over **the hub edition's** members' `sha256`s,
/// in member order.
///
/// **That edition alone, and not every member of every edition.** Folding all
/// editions in would change a work's identity whenever one was added — and
/// adding editions is the entire point of this project. A rescan would then see
/// the work vanish and a new one appear, taking its progress and its notes with
/// it. The hub is fixed when the work is created.
///
/// # Errors
///
/// Returns an error if the document's `hub` names no edition it holds, with the
/// message naming the hub key that was not found.
pub fn content_key(sidecar: &Sidecar) -> Result<String, String> {
    let hub = sidecar
        .editions
        .get(&sidecar.hub)
        .ok_or_else(|| format!("hub `{}` names no edition in this document", sidecar.hub))?;

    let mut hasher = Sha256::new();
    for member in &hub.members {
        hasher.update(member.sha256.as_bytes());
        // A separator, so two members whose hashes concatenate the same way as
        // one longer pair cannot collide. Hex digests are fixed-width today and
        // this does not rely on that staying true.
        hasher.update(b"\n");
    }
    Ok(hasher.finalize().iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    }))
}

#[cfg(test)]
mod tests;
