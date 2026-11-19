//! JPEG XL, read wherever `image` reads a page.
//!
//! `image` has no JPEG XL decoder of its own and takes one through its hooks,
//! which are process-wide: once [`register`] has run, every
//! `ImageReader::with_guessed_format` in the process decodes JPEG XL. The
//! grids a decode renders into count against the caller's `max_alloc`; the
//! metadata boxes jxl-oxide inflates do not, so a page whose boxes would
//! inflate past `METADATA_CAP` is refused before jxl-oxide reads it.

use std::io::{Cursor, Read as _};
use std::sync::{Arc, Once};

use image::error::{DecodingError, ImageFormatHint, LimitError, LimitErrorKind};
use image::{ColorType, ImageDecoder, ImageError, ImageResult, Limits};
use jxl_oxide::integration::JxlDecoder;
use jxl_oxide::jpeg_bitstream::JpegBitstreamHeader;
use jxl_oxide::{AllocTracker, JxlImage, JxlThreadPool};
use jxl_oxide_common::Bundle as _;
use taiyaku_read::MAX_DECODE_ALLOC;

use crate::fingerprint_pass::priority;
use crate::source::{JXL_CODESTREAM, JXL_CONTAINER};

/// The most a page's metadata boxes may inflate to, all of them together.
/// jxl-oxide inflates them outside every limit a caller sets, so a page of a
/// few KB could otherwise hold gigabytes. A page's metadata is its Exif and
/// XMP and, for a recompressed JPEG, the marker segments and trailing bytes a
/// rebuild puts back; a JPEG keeps each segment under 64 KiB, so 16 MiB is
/// room for far more than a page carries.
const METADATA_CAP: usize = 16 * 1024 * 1024;

/// What a decode on this thread renders on. jxl-oxide's default is rayon's
/// shared pool, whose threads keep the priority of the thread that first used
/// it, for every caller after. A lowered thread's work there would run at
/// normal priority, and a lowered thread that started the pool would leave
/// every reader's decode at its priority, so a lowered thread works alone.
fn render_pool() -> JxlThreadPool {
    if priority::thread_is_lowered() {
        JxlThreadPool::none()
    } else {
        JxlThreadPool::rayon_global()
    }
}

/// Registers the JPEG XL decoder for `"jxl"` and a detection hook for each
/// of its two signatures.
///
/// Once per process however often it is called: `image` refuses a second
/// decoding hook but appends every detection hook it is given, so a repeated
/// registration would grow the list each guess walks.
pub fn register() {
    static REGISTERED: Once = Once::new();
    REGISTERED.call_once(|| {
        let registered = image::hooks::register_decoding_hook(
            "jxl".into(),
            Box::new(|mut reader| {
                // Read whole: the check that the first frame is all there
                // needs every byte, and the decoder reads the page again.
                let mut page = Vec::new();
                reader.read_to_end(&mut page)?;
                // Before jxl-oxide sees the page: it inflates the boxes ahead
                // of the codestream while it reads the header.
                if !metadata_fits(&page) {
                    return Err(ImageError::Limits(LimitError::from_kind(
                        LimitErrorKind::InsufficientMemory,
                    )));
                }
                let page: Arc<[u8]> = page.into();
                let inner =
                    JxlDecoder::with_thread_pool(Cursor::new(Arc::clone(&page)), render_pool())?;
                Ok(Box::new(Decoder {
                    page,
                    inner,
                    max_alloc: usize::MAX,
                }))
            }),
        );
        assert!(
            registered,
            "a decoder for \"jxl\" was registered before this one, so JPEG XL pages \
             would decode without its metadata cap, cut-short refusal and limits"
        );
        for signature in [JXL_CODESTREAM, JXL_CONTAINER] {
            image::hooks::register_format_detection_hook("jxl".into(), signature, None);
        }
    });
}

/// jxl-oxide's decoder, which renders the first frame and holds the caller's
/// limits, with two changes. A render that outgrows `max_alloc` is refused as
/// `ImageError::Limits`, the kind every other decode over its limits gives,
/// so a caller tells "too big for the budget" from "broken" by one match. A
/// page that ends before its first frame does is refused, where jxl-oxide
/// would draw the part that arrived.
struct Decoder {
    page: Arc<[u8]>,
    inner: JxlDecoder<Cursor<Arc<[u8]>>>,
    /// The caller's `max_alloc` as `set_limits` last passed it on, so the
    /// first-frame check loads under the same limit the render does.
    max_alloc: usize,
}

impl ImageDecoder for Decoder {
    fn dimensions(&self) -> (u32, u32) {
        self.inner.dimensions()
    }

    fn color_type(&self) -> ColorType {
        self.inner.color_type()
    }

    fn icc_profile(&mut self) -> ImageResult<Option<Vec<u8>>> {
        self.inner.icc_profile()
    }

    fn exif_metadata(&mut self) -> ImageResult<Option<Vec<u8>>> {
        self.inner.exif_metadata().map_err(out_of_memory_as_limits)
    }

    fn set_limits(&mut self, limits: Limits) -> ImageResult<()> {
        let max_alloc = limits.max_alloc.map_or(usize::MAX, |bytes| {
            usize::try_from(bytes).unwrap_or(usize::MAX)
        });
        self.inner.set_limits(limits)?;
        self.max_alloc = max_alloc;
        Ok(())
    }

    fn read_image(self, buf: &mut [u8]) -> ImageResult<()> {
        whole_first_frame(&self.page, self.max_alloc)?;
        self.inner.read_image(buf).map_err(out_of_memory_as_limits)
    }

    fn read_image_boxed(self: Box<Self>, buf: &mut [u8]) -> ImageResult<()> {
        (*self).read_image(buf)
    }
}

/// Refuses a page whose first frame is not all there. A part drawn as the page
/// would be resized, cached and served as if it were the page, so a page cut
/// short is refused like any other broken one. jxl-oxide counts a keyframe as
/// loaded only once every byte of it has arrived.
fn whole_first_frame(page: &[u8], max_alloc: usize) -> ImageResult<()> {
    let decoding = |e: Box<dyn std::error::Error + Send + Sync>| {
        ImageError::Decoding(DecodingError::new(
            ImageFormatHint::PathExtension("jxl".into()),
            e,
        ))
    };
    let loaded = read_page(page, max_alloc).map_err(|e| out_of_memory_as_limits(decoding(e)))?;
    if loaded.num_loaded_keyframes() == 0 {
        return Err(decoding(
            "incomplete page: it ends before its first frame does".into(),
        ));
    }
    Ok(())
}

/// A whole page read by jxl-oxide, its allocations counted against `max_alloc`.
fn read_page(
    page: &[u8],
    max_alloc: usize,
) -> Result<JxlImage, Box<dyn std::error::Error + Send + Sync>> {
    JxlImage::builder()
        .pool(render_pool())
        .alloc_tracker(AllocTracker::with_limit(max_alloc))
        .read(page)
}

/// jxl-oxide reports its allocation tracker's refusal as a decoding error
/// whose cause, some levels down, is `jxl_grid::OutOfMemory`. Found by type,
/// never by message, so a reworded error cannot turn a refusal into a
/// broken page.
fn out_of_memory_as_limits(error: ImageError) -> ImageError {
    let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(e) = cause {
        if e.is::<jxl_grid::OutOfMemory>() {
            return ImageError::Limits(LimitError::from_kind(LimitErrorKind::InsufficientMemory));
        }
        cause = e.source();
    }
    error
}

/// The JPEG a page was made from, where the page carries the data to rebuild
/// it and the rebuild succeeds; `None` otherwise.
///
/// The rebuilt file is the original's own bytes, so serving it costs no
/// re-encode and adds no second generation of loss. Its allocations count
/// against `MAX_DECODE_ALLOC`, the ceiling every decode here keeps.
pub(crate) fn rebuilt_jpeg(page: &[u8]) -> Option<Vec<u8>> {
    if !metadata_fits(page) {
        return None;
    }
    let ceiling = usize::try_from(MAX_DECODE_ALLOC).unwrap_or(usize::MAX);
    let image = read_page(page, ceiling).ok()?;
    let mut jpeg = Vec::new();
    image.reconstruct_jpeg(&mut jpeg).ok()?;
    Some(jpeg)
}

/// Whether everything jxl-oxide would inflate from the page stays within
/// `METADATA_CAP` together: each `brob` box, and the compressed stream after
/// the header of the `jbrd` boxes, which jxl-oxide reads as one payload.
/// Counted by inflating into nothing, so the check holds a brotli window, not
/// the output. A bare codestream has no boxes.
fn metadata_fits(page: &[u8]) -> bool {
    if !page.starts_with(JXL_CONTAINER) {
        return true;
    }
    let mut left = METADATA_CAP;
    let mut reconstruction = Vec::new();
    for (kind, payload) in boxes(page) {
        if kind == *b"brob" {
            let spent = inflated_length(payload.get(4..).unwrap_or_default(), left);
            if spent > left {
                return false;
            }
            left -= spent;
        } else if kind == *b"jbrd" {
            reconstruction.extend_from_slice(payload);
        }
    }
    reconstruction_stream(&reconstruction)
        .is_none_or(|stream| inflated_length(stream, left) <= left)
}

/// A container's boxes as (type, payload), in order, the last cut where the
/// page ends. Stops at a box header jxl-oxide would refuse, which ends its
/// reading there too.
fn boxes(page: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut rest = page;
    std::iter::from_fn(move || {
        let size = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?);
        let kind: [u8; 4] = rest.get(4..8)?.try_into().ok()?;
        let (header, size) = match size {
            0 => (8, rest.len()),
            1 => (
                16,
                usize::try_from(u64::from_be_bytes(rest.get(8..16)?.try_into().ok()?)).ok()?,
            ),
            n => (8, usize::try_from(n).ok()?),
        };
        if size < header {
            return None;
        }
        let end = size.min(rest.len());
        let payload = rest.get(header..end)?;
        rest = &rest[end..];
        Some((kind, payload))
    })
}

/// The compressed stream of a `jbrd` payload: what follows its header, from
/// the next whole byte, as jxl-oxide reads it. `None` where the header does
/// not parse, and jxl-oxide then inflates nothing.
fn reconstruction_stream(payload: &[u8]) -> Option<&[u8]> {
    let mut bits = jxl_bitstream::Bitstream::new(payload);
    JpegBitstreamHeader::parse(&mut bits, ()).ok()?;
    bits.zero_pad_to_byte().ok()?;
    payload.get(bits.num_read_bits() / 8..)
}

/// How many bytes a brotli stream inflates to, counted no further than one
/// past `limit`, so a bomb is stopped as soon as it is one. A stream that
/// breaks off counts what came before, as much as jxl-oxide would keep.
fn inflated_length(compressed: &[u8], limit: usize) -> usize {
    let mut stream = brotli_decompressor::Decompressor::new(compressed, 4096);
    let mut chunk = [0; 4096];
    let mut total = 0;
    while total <= limit {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => total += n,
        }
    }
    total
}

#[cfg(test)]
mod process_tests;
#[cfg(test)]
mod tests;
