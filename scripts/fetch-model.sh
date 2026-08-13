#!/usr/bin/env bash
#
# Fetch comic-text-detector's ONNX weights for the L2 phase-3 measurement.
#
# The weights are **not committed**: 95 MB in every clone forever, for a module
# that is `#[cfg(test)]` and skips unless you ask for it. See
# `docs/superpowers/specs/2026-08-11-l2-detector-design.md` §4.2.
#
# --- where these come from, and why not from the obvious place ---------------
#
# From the `zyddnys/manga-image-translator` release, which is a GPL-3.0 project.
# The Hugging Face mirror `mayocream/comic-text-detector-onnx` serves the
# **same bytes** — verified, identical sha256 — while tagging them `apache-2.0`.
# It is the first hit when you search for this model and it is mislabelled, so
# the authoritative source is named here explicitly.
#
# --- the licence obligation this creates -------------------------------------
#
# One third of the training data is Manga109-s, the 87-volume commercially
# approved subset of Manga109 (**not** the academic-only Manga109 whose "solely
# for academic purposes" clause a reader meets first on that site). Its terms
# permit publishing results "including pre-trained models" on one condition:
#
#   "the use of the Manga109-s dataset must be indicated clearly within the
#    published work"
#
# So anything this project publishes carrying these weights must say so. See
# `docs/notes.md`, "comic-text-detector through `tract`".
#
# --- the checksum -------------------------------------------------------------
#
# A GitHub release asset can be replaced in place by whoever owns the
# repository, and upstream publishes no checksum of its own. The digest below
# was computed here from the artefact this measurement was taken against. Bump
# it together with the URL, by downloading and re-computing — never by copying a
# value out of a README or an issue. This is the same rule the nextest download
# in `.forgejo/workflows/ci.yml` follows, for the same reason.

set -euo pipefail

URL="https://github.com/zyddnys/manga-image-translator/releases/download/beta-0.2.1/comictextdetector.pt.onnx"
SHA256="1a86ace74961413cbd650002e7bb4dcec4980ffa21b2f19b86933372071d718f"
DEST="${1:-models/comictextdetector.pt.onnx}"

if [ -f "$DEST" ] && echo "$SHA256  $DEST" | sha256sum -c - >/dev/null 2>&1; then
  echo "taiyaku: $DEST is already present and verified"
else
  mkdir -p "$(dirname "$DEST")"
  echo "taiyaku: fetching 95 MB from the manga-image-translator release..."
  curl -fsSL -o "$DEST" "$URL"
  echo "$SHA256  $DEST" | sha256sum -c -
fi

cat <<EOF

taiyaku: verified. To run the phase-3 measurement:

  TAIYAKU_CORPUS="\$PWD/corpus" TAIYAKU_MODEL="\$PWD/$DEST" \\
    cargo test --release -p taiyaku-cli model_experiment -- --nocapture

EOF
