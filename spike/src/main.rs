//! Throwaway spike. Not production code, not tested, not linted to workspace
//! standards. It exists to answer four questions against the real corpus:
//!
//!   1. What is actually inside the archives — format, resolution, entry order?
//!   2. What does decode cost, and does scaled JPEG decode help?
//!   3. Which structural hash separates same-page from different-page best?
//!   4. Does the similarity signal survive colour -> mono and JP -> EN?
//!
//! Findings go in docs/notes.md; this directory then gets deleted.
//!
//! Usage:
//!   spike inventory <archive>...
//!   spike hash <archive> [--algo dhash|phash] [--scale N]
//!   spike matrix <archive-a> <archive-b> [--algo dhash|phash] [--scale N]

use std::{
    collections::BTreeMap,
    env,
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::Instant,
};

use image::{ImageReader, imageops::FilterType};
use rayon::prelude::*;

// ---------------------------------------------------------------------------
// archive reading
// ---------------------------------------------------------------------------

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "avif"];

struct Page {
    name: String,
    bytes: Vec<u8>,
}

/// One token of a natural-order sort key.
///
/// Numbers are `f64` rather than integers so that a digit run followed by
/// `.digits` parses as a single decimal. That is what makes `c005` sort before
/// `c005.5`: token-wise natural sort alone still gets it wrong, because it sees
/// `5`, `.`, `5` and compares the separator against `.cbz`.
#[derive(Debug)]
enum Tok {
    Num(f64),
    Text(String),
}

fn tokenize(s: &str) -> Vec<Tok> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            // A decimal point only counts when digits follow it.
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Tok::Num(text.parse().unwrap_or(f64::MAX)));
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Tok::Text(
                chars[start..i].iter().collect::<String>().to_lowercase(),
            ));
        }
    }
    out
}

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (ta, tb) = (tokenize(a), tokenize(b));
    for pair in ta.iter().zip(tb.iter()) {
        let ord = match pair {
            (Tok::Num(x), Tok::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
            (Tok::Text(x), Tok::Text(y)) => x.cmp(y),
            // Digits sort before letters, so `9.jpg` precedes `cover.jpg`.
            (Tok::Num(_), Tok::Text(_)) => Ordering::Less,
            (Tok::Text(_), Tok::Num(_)) => Ordering::Greater,
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    ta.len().cmp(&tb.len())
}

fn is_image(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Read an edition member: either a cbz archive or a directory of page images.
///
/// Directories are not a fallback case — three of the four raw series in the
/// corpus are stored that way rather than as archives.
fn read_pages(path: &Path) -> Vec<Page> {
    if path.is_dir() {
        read_pages_dir(path)
    } else {
        read_pages_zip(path)
    }
}

fn read_pages_dir(root: &Path) -> Vec<Page> {
    let mut names: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
        for entry in entries.flatten() {
            let p = entry.path();
            let base = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            if base.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if is_image(&base) {
                names.push(p);
            }
        }
    }
    names.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    names
        .into_iter()
        .map(|p| Page {
            name: p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .to_string(),
            bytes: std::fs::read(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display())),
        })
        .collect()
}

/// Read every image entry of a cbz into memory, in natural-name order.
///
/// Sorted by name because zip central-directory order is not guaranteed to be
/// page order.
fn read_pages_zip(path: &Path) -> Vec<Page> {
    let file = File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let mut zip =
        zip::ZipArchive::new(file).unwrap_or_else(|e| panic!("zip {}: {e}", path.display()));

    let mut names: Vec<String> = (0..zip.len())
        .filter_map(|i| {
            let entry = zip.by_index(i).ok()?;
            if !entry.is_file() {
                return None;
            }
            let name = entry.name().to_string();
            if name.starts_with("__MACOSX") || name.rsplit('/').next()?.starts_with('.') {
                return None;
            }
            is_image(&name).then_some(name)
        })
        .collect();
    names.sort_by(|a, b| natural_cmp(a, b));

    names
        .into_iter()
        .map(|name| {
            let mut entry = zip.by_name(&name).expect("entry vanished");
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes).expect("read entry");
            Page { name, bytes }
        })
        .collect()
}

/// Non-image entries, so we can see what else rides along (ComicInfo.xml etc).
fn read_extras(path: &Path) -> Vec<(String, u64)> {
    if path.is_dir() {
        return Vec::new();
    }
    let file = File::open(path).expect("open");
    let mut zip = zip::ZipArchive::new(file).expect("zip");
    (0..zip.len())
        .filter_map(|i| {
            let entry = zip.by_index(i).ok()?;
            if !entry.is_file() {
                return None;
            }
            let name = entry.name().to_string();
            let is_image = Path::new(&name)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            (!is_image).then_some((name, entry.size()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// hashing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Algo {
    /// Difference hash: 9x8 luma, compare each pixel to its right neighbour.
    DHash,
    /// Perceptual hash: 32x32 luma, DCT-II, low-frequency 8x8 against its median.
    PHash,
}

#[derive(Debug, Clone)]
struct Config {
    algo: Algo,
    /// Hash grid edge: `size * size` bits for dhash, `size * size - 1` for phash.
    size: usize,
    /// Hash gradient magnitude rather than luma.
    edges: bool,
    /// Crop each page to its inked area before hashing.
    crop: bool,
    /// Mirror pages horizontally before hashing (tests flipped releases).
    flip: bool,
    /// JPEG DCT decode scale denominator.
    scale: u32,
}

impl Algo {
    fn parse(s: &str) -> Self {
        match s {
            "dhash" => Algo::DHash,
            "phash" => Algo::PHash,
            other => panic!("unknown algo {other}"),
        }
    }
}

/// Decode to luma. `scale` requests a JPEG DCT-scaled decode by that
/// denominator (1, 2, 4, 8); non-JPEG input ignores it and decodes in full.
fn decode_luma(bytes: &[u8], scale: u32) -> image::GrayImage {
    if scale > 1
        && let Some(img) = decode_jpeg_scaled(bytes, scale)
    {
        return img;
    }
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .expect("guess format")
        .decode()
        .expect("decode")
        .to_luma8()
}

/// Placeholder for the DCT-scaled JPEG path — question 2 of the spike.
/// Returns `None` until the `image` API for it is confirmed, so `--scale`
/// currently measures the full-decode baseline.
fn decode_jpeg_scaled(_bytes: &[u8], _scale: u32) -> Option<image::GrayImage> {
    None
}

/// A hash of arbitrary bit length, so hash resolution can be swept.
type Hash = Vec<u64>;

fn pack(bits: impl Iterator<Item = bool>) -> Hash {
    let mut out = Vec::new();
    for (i, b) in bits.enumerate() {
        if i % 64 == 0 {
            out.push(0);
        }
        if b {
            let last = out.len() - 1;
            out[last] |= 1 << (i % 64);
        }
    }
    out
}

/// Sobel gradient magnitude.
///
/// The point of this for the corpus: colourisation repaints large flat regions,
/// which moves luma a great deal, but it leaves the linework and panel borders
/// exactly where they were. Hashing gradients rather than luma should therefore
/// survive a mono -> colour re-release far better. That is the hypothesis the
/// `--edges` flag exists to test.
fn sobel(img: &image::GrayImage) -> image::GrayImage {
    let (w, h) = img.dimensions();
    let mut out = image::GrayImage::new(w, h);
    let at = |x: i32, y: i32| -> i32 {
        let x = x.clamp(0, w as i32 - 1) as u32;
        let y = y.clamp(0, h as i32 - 1) as u32;
        img.get_pixel(x, y).0[0] as i32
    };
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let gx = -at(x - 1, y - 1) - 2 * at(x - 1, y) - at(x - 1, y + 1)
                + at(x + 1, y - 1)
                + 2 * at(x + 1, y)
                + at(x + 1, y + 1);
            let gy = -at(x - 1, y - 1) - 2 * at(x, y - 1) - at(x + 1, y - 1)
                + at(x - 1, y + 1)
                + 2 * at(x, y + 1)
                + at(x + 1, y + 1);
            let mag = ((gx * gx + gy * gy) as f64).sqrt().min(255.0) as u8;
            out.put_pixel(x as u32, y as u32, image::Luma([mag]));
        }
    }
    out
}

/// Trim uniform border margin down to the page content.
///
/// Editions of the same volume are scanned and cropped differently, so their
/// aspect ratios disagree — Kaguya's raw is 0.703 against the English release's
/// 0.667. Resizing both to a fixed hash grid stretches them by different amounts
/// and the same artwork lands in different cells. Cropping to the inked area
/// first removes the part of that difference that is just margin.
fn autocrop(img: &image::GrayImage) -> image::GrayImage {
    let work = image::imageops::resize(img, 200, 280, FilterType::Triangle);
    let (w, h) = work.dimensions();

    let row_mean = |y: u32| -> f64 {
        (0..w).map(|x| work.get_pixel(x, y).0[0] as f64).sum::<f64>() / w as f64
    };
    let col_mean = |x: u32| -> f64 {
        (0..h).map(|y| work.get_pixel(x, y).0[0] as f64).sum::<f64>() / h as f64
    };

    // Background is whatever the outermost edges are, so this works on a dark
    // colour spread as well as on white manga stock.
    let bg = (row_mean(0) + row_mean(h - 1) + col_mean(0) + col_mean(w - 1)) / 4.0;
    let tol = 8.0;
    // Never trim more than a quarter from a side: a page that is genuinely
    // almost blank must not collapse to nothing.
    let (max_x, max_y) = (w / 4, h / 4);

    let mut top = 0;
    while top < max_y && (row_mean(top) - bg).abs() < tol {
        top += 1;
    }
    let mut bottom = h - 1;
    while bottom > h - 1 - max_y && (row_mean(bottom) - bg).abs() < tol {
        bottom -= 1;
    }
    let mut left = 0;
    while left < max_x && (col_mean(left) - bg).abs() < tol {
        left += 1;
    }
    let mut right = w - 1;
    while right > w - 1 - max_x && (col_mean(right) - bg).abs() < tol {
        right -= 1;
    }

    if right <= left || bottom <= top {
        return work;
    }
    // Map the crop box back onto the full-resolution image.
    let (fw, fh) = img.dimensions();
    let sx = fw as f64 / w as f64;
    let sy = fh as f64 / h as f64;
    let x0 = (left as f64 * sx) as u32;
    let y0 = (top as f64 * sy) as u32;
    let cw = (((right - left + 1) as f64) * sx) as u32;
    let ch = (((bottom - top + 1) as f64) * sy) as u32;
    image::imageops::crop_imm(img, x0, y0, cw.min(fw - x0), ch.min(fh - y0)).to_image()
}

/// Shared preprocessing: optional crop to content, luma, optional edge map, at a
/// working resolution well above the hash grid so the gradient operator has
/// something to bite on.
fn preprocess(img: &image::GrayImage, edges: bool, crop: bool, flip: bool) -> image::GrayImage {
    let cropped = if crop { autocrop(img) } else { img.clone() };
    let base = if flip { image::imageops::flip_horizontal(&cropped) } else { cropped };
    if !edges {
        return base;
    }
    let work = image::imageops::resize(&base, 128, 128, FilterType::Lanczos3);
    sobel(&work)
}

/// Difference hash on an `n x n` grid: compare each cell to its right neighbour.
fn dhash(img: &image::GrayImage, n: u32) -> Hash {
    let small = image::imageops::resize(img, n + 1, n, FilterType::Lanczos3);
    pack((0..n).flat_map(|y| {
        let small = &small;
        (0..n).map(move |x| small.get_pixel(x, y).0[0] > small.get_pixel(x + 1, y).0[0])
    }))
}

/// Perceptual hash: DCT-II over a `4n x 4n` luma grid, keeping the `n x n`
/// low-frequency corner, each coefficient thresholded against their median.
fn phash(img: &image::GrayImage, n: usize) -> Hash {
    let size = n * 4;
    let small = image::imageops::resize(img, size as u32, size as u32, FilterType::Lanczos3);

    let mut input = vec![vec![0f64; size]; size];
    for (y, row) in input.iter_mut().enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            *v = small.get_pixel(x as u32, y as u32).0[0] as f64;
        }
    }

    // Separable DCT-II, naive and O(size^3). Irrelevant next to JPEG decode.
    let mut cos = vec![vec![0f64; size]; size];
    for (k, row) in cos.iter_mut().enumerate() {
        for (m, c) in row.iter_mut().enumerate() {
            *c = ((2.0 * m as f64 + 1.0) * k as f64 * std::f64::consts::PI / (2.0 * size as f64))
                .cos();
        }
    }
    let mut rows = vec![vec![0f64; size]; size];
    for y in 0..size {
        for k in 0..size {
            rows[y][k] = (0..size).map(|m| input[y][m] * cos[k][m]).sum();
        }
    }
    let mut dct = vec![vec![0f64; size]; size];
    for x in 0..size {
        for k in 0..size {
            dct[k][x] = (0..size).map(|m| rows[m][x] * cos[k][m]).sum();
        }
    }

    // Skip the DC term: it encodes only overall brightness, which is exactly
    // what differs between a mono scan and its colourised re-release.
    let mut coeffs = Vec::with_capacity(n * n - 1);
    for y in 0..n {
        for x in 0..n {
            if (x, y) != (0, 0) {
                coeffs.push(dct[y][x]);
            }
        }
    }
    let mut sorted = coeffs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];

    pack(coeffs.iter().map(|c| *c > median))
}

fn hash_pages(pages: &[Page], cfg: &Config) -> Vec<Hash> {
    pages
        .par_iter()
        .map(|p| {
            let luma = decode_luma(&p.bytes, cfg.scale);
            let prepped = preprocess(&luma, cfg.edges, cfg.crop, cfg.flip);
            match cfg.algo {
                Algo::DHash => dhash(&prepped, cfg.size as u32),
                Algo::PHash => phash(&prepped, cfg.size),
            }
        })
        .collect()
}

fn similarity(a: &Hash, b: &Hash) -> f64 {
    let bits: u32 = a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum();
    let total = (a.len() * 64) as f64;
    1.0 - bits as f64 / total
}

// ---------------------------------------------------------------------------
// commands
// ---------------------------------------------------------------------------

/// Do the halves of a joined double-page spread match two separate pages?
///
/// Edition B joins spreads that edition A splits, so a landscape page in B
/// should be two portrait pages of A side by side. If that is true, cutting the
/// wide page down the middle and hashing each half should find those two pages,
/// and the order tells us which half is which in reading order. If it is false,
/// the whole fractional-counterpart idea is wrong and spreads need a real
/// geometric transform instead of a crop.
fn cmd_spreads(a_path: &Path, b_path: &Path, cfg: &Config) {
    let a_pages = read_pages(a_path);
    let b_pages = read_pages(b_path);
    let a = hash_pages(&a_pages, cfg);

    println!("# b_idx  half   best_a   sim    runner_up  sim");
    let mut found = 0;
    for (j, page) in b_pages.iter().enumerate() {
        let img = decode_luma(&page.bytes, cfg.scale);
        let (w, h) = img.dimensions();
        if w <= h {
            continue;
        }
        found += 1;
        let mid = w / 2;
        for (name, x0) in [("left", 0u32), ("right", mid)] {
            let half = image::imageops::crop_imm(&img, x0, 0, mid, h).to_image();
            let prepped = preprocess(&half, cfg.edges, cfg.crop, false);
            let hash = match cfg.algo {
                Algo::DHash => dhash(&prepped, cfg.size as u32),
                Algo::PHash => phash(&prepped, cfg.size),
            };
            let mut scored: Vec<(usize, f64)> = a
                .iter()
                .enumerate()
                .map(|(i, ha)| (i, similarity(&hash, ha)))
                .collect();
            scored.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
            println!(
                "{j:>7}  {name:<5} {:>6}  {:.3}  {:>9}  {:.3}",
                scored[0].0, scored[0].1, scored[1].0, scored[1].1
            );
        }
    }
    eprintln!("{found} landscape pages in B, {} pages in A", a.len());
}

fn cmd_inventory(paths: &[PathBuf]) {
    for path in paths {
        let started = Instant::now();
        let pages = read_pages(path);
        let extras = read_extras(path);
        println!("== {}", path.display());
        println!(
            "   {} image entries, read in {:?}",
            pages.len(),
            started.elapsed()
        );

        let mut formats: BTreeMap<String, usize> = BTreeMap::new();
        let mut dims: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for p in &pages {
            let reader = ImageReader::new(Cursor::new(&p.bytes))
                .with_guessed_format()
                .expect("guess");
            let fmt = reader
                .format()
                .map(|f| format!("{f:?}"))
                .unwrap_or_else(|| "unknown".into());
            let d = reader.into_dimensions().expect("dimensions");
            *formats.entry(fmt).or_default() += 1;
            *dims.entry(d).or_default() += 1;
        }

        println!("   formats:");
        for (f, n) in &formats {
            println!("     {f:<8} {n}");
        }
        println!("   distinct resolutions: {}", dims.len());
        let mut by_count: Vec<_> = dims.iter().collect();
        by_count.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for ((w, h), n) in by_count.iter().take(6) {
            let orientation = if w > h {
                "landscape (spread?)"
            } else {
                "portrait"
            };
            println!("     {w}x{h:<6} {n:>4}  {orientation}");
        }
        if by_count.len() > 6 {
            println!("     ... {} more", by_count.len() - 6);
        }

        println!("   entry names, first 3 and last 2:");
        for p in pages.iter().take(3) {
            println!("     {}", p.name);
        }
        println!("     ...");
        let tail: Vec<_> = pages.iter().rev().take(2).collect();
        for p in tail.iter().rev() {
            println!("     {}", p.name);
        }

        if extras.is_empty() {
            println!("   no non-image entries");
        } else {
            println!("   non-image entries:");
            for (name, size) in &extras {
                println!("     {name} ({size} bytes)");
            }
        }
        println!();
    }
}

/// Length of the longest non-decreasing subsequence. O(n log n).
fn longest_non_decreasing(seq: &[usize]) -> usize {
    let mut tails: Vec<usize> = Vec::new();
    for &v in seq {
        // upper_bound: first tail strictly greater than v.
        let pos = tails.partition_point(|&t| t <= v);
        if pos == tails.len() {
            tails.push(v);
        } else {
            tails[pos] = v;
        }
    }
    tails.len()
}

fn hex(h: &Hash) -> String {
    h.iter().map(|w| format!("{w:016x}")).collect()
}

fn cmd_hash(path: &Path, cfg: &Config) {
    let t0 = Instant::now();
    let pages = read_pages(path);
    let read = t0.elapsed();

    let t1 = Instant::now();
    let hashes = hash_pages(&pages, cfg);
    let hashed = t1.elapsed();

    for (i, (p, h)) in pages.iter().zip(&hashes).enumerate() {
        println!("{i:>4}  {}  {}", hex(h), p.name);
    }
    eprintln!(
        "\n{} pages | read {read:?} | decode+hash {hashed:?} ({:?}/page) | {cfg:?}",
        pages.len(),
        hashed / pages.len().max(1) as u32
    );
}

/// `b_paths` is a list because an edition is not a file: the fan translation is
/// one volume's worth of pages spread over seven chapter archives. Both sides
/// flatten to a linear page sequence before anything else happens, which is the
/// edition model from docs/notes.md exercised for real.
fn cmd_matrix(a_path: &Path, b_paths: &[PathBuf], cfg: &Config) {
    let a_pages = read_pages(a_path);
    // Order the members ourselves rather than trusting argv: a shell glob sorts
    // lexicographically and so puts c005.5 before c005.
    let mut b_paths: Vec<PathBuf> = b_paths.to_vec();
    b_paths.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    let b_paths = &b_paths;

    let mut b_pages = Vec::new();
    let mut boundaries = Vec::new();
    for p in b_paths {
        boundaries.push((b_pages.len(), p.file_name().unwrap().to_string_lossy().to_string()));
        b_pages.extend(read_pages(p));
    }

    let t = Instant::now();
    let mut cfg_a = cfg.clone();
    cfg_a.flip = false;
    let a = hash_pages(&a_pages, &cfg_a);
    let b = hash_pages(&b_pages, cfg);
    let bits = a.first().map(|h| h.len() * 64).unwrap_or(0);
    eprintln!(
        "hashed {} + {} pages in {:?} | {cfg:?} | {bits} bits",
        a.len(),
        b.len(),
        t.elapsed()
    );
    if b_paths.len() > 1 {
        eprintln!("B is {} archives concatenated:", b_paths.len());
        for (start, name) in &boundaries {
            eprintln!("  b_idx {start:>4}  {name}");
        }
    }

    // Cost of the full pairwise matrix on its own, to size the question of
    // whether pairing could avoid comparing every page against every page.
    let t_mat = Instant::now();
    let mut matrix = vec![0f32; a.len() * b.len()];
    for (i, ha) in a.iter().enumerate() {
        for (j, hb) in b.iter().enumerate() {
            matrix[i * b.len() + j] = similarity(ha, hb) as f32;
        }
    }
    let mat_elapsed = t_mat.elapsed();
    eprintln!(
        "full {}x{} matrix in {mat_elapsed:?} ({} comparisons)",
        a.len(),
        b.len(),
        a.len() * b.len()
    );

    // How far apart are true matches in bit terms? Exact-hash lookup only works
    // if the answer is ~0.
    let mut bit_dists: Vec<u32> = Vec::new();
    for (i, ha) in a.iter().enumerate() {
        let best = b
            .iter()
            .map(|hb| {
                ha.iter()
                    .zip(hb)
                    .map(|(x, y)| (x ^ y).count_ones())
                    .sum::<u32>()
            })
            .min()
            .unwrap_or(0);
        bit_dists.push(best);
        let _ = i;
    }
    bit_dists.sort_unstable();
    eprintln!(
        "bits differing at the best match ({} total): min {} | median {} | p90 {}",
        bits,
        bit_dists.first().copied().unwrap_or(0),
        bit_dists[bit_dists.len() / 2],
        bit_dists[bit_dists.len() * 9 / 10]
    );
    eprintln!(
        "  exact-hash lookup would find {}/{} pages",
        bit_dists.iter().filter(|d| **d == 0).count(),
        bit_dists.len()
    );

    println!("# A={} ({} pages)", a_path.display(), a.len());
    println!("# B={} archive(s), {} pages", b_paths.len(), b.len());
    println!("# a_idx  best_b  sim    runner_up  sim    margin  identity?");

    let mut identity_hits = 0usize;
    let mut margins = Vec::new();
    let mut best_matches: Vec<usize> = Vec::new();
    let mut best_sims: Vec<f64> = Vec::new();
    // Separation is the number that actually matters. Argmax accuracy can look
    // fine while correct and incorrect pairs sit almost on top of each other,
    // and a DP pass over a matrix with no separation has nothing to work with.
    let mut diagonal = Vec::new();
    let mut off_diagonal = Vec::new();

    for (i, ha) in a.iter().enumerate() {
        let mut scored: Vec<(usize, f64)> = b
            .iter()
            .enumerate()
            .map(|(j, hb)| (j, similarity(ha, hb)))
            .collect();
        for &(j, s) in &scored {
            if j == i {
                diagonal.push(s);
            } else {
                off_diagonal.push(s);
            }
        }
        scored.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
        let (best_j, best_s) = scored[0];
        let (run_j, run_s) = scored.get(1).copied().unwrap_or((0, 0.0));
        let margin = best_s - run_s;
        margins.push(margin);
        best_matches.push(best_j);
        best_sims.push(best_s);
        let identity = best_j == i;
        if identity {
            identity_hits += 1;
        }
        println!(
            "{i:>6}  {best_j:>6}  {best_s:.3}  {run_j:>9}  {run_s:.3}  {margin:.3}   {}",
            if identity { "yes" } else { "NO" }
        );
    }

    // Distribution of the best match per page. Pages with a real counterpart
    // should pile up high, pages without should pile up low, and the floor is
    // the valley between them. Printing the shape rather than a single mean is
    // the point: a constant threshold is only defensible if the valley is in
    // the same place across series.
    eprintln!("best-match similarity distribution:");
    let mut buckets = [0usize; 20];
    for s in &best_sims {
        let b = ((s * 20.0) as usize).min(19);
        buckets[b] += 1;
    }
    for (i, n) in buckets.iter().enumerate() {
        if *n > 0 {
            let lo = i as f64 / 20.0;
            eprintln!(
                "  {lo:.2}-{:.2} {:>4} {}",
                lo + 0.05,
                n,
                "#".repeat((*n as f64 / 2.0).ceil() as usize)
            );
        }
    }

    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let mean_margin = mean(&margins);

    // With unequal page counts there is no identity diagonal to score against,
    // so the usable proxy is how close the argmax sequence is to monotonic. A
    // real alignment is a staircase; noise is not. This is the fraction of
    // pages that lie on the longest non-decreasing run of best matches, which
    // is roughly the ceiling a monotonic DP pass could recover.
    let lis = longest_non_decreasing(&best_matches);
    eprintln!(
        "argmax path: {lis}/{} pages lie on a monotonic staircase ({:.1}%)",
        best_matches.len(),
        100.0 * lis as f64 / best_matches.len().max(1) as f64
    );
    eprintln!(
        "argmax == index for {identity_hits}/{} pages ({:.1}%) | mean margin over runner-up {mean_margin:.3}",
        a.len(),
        100.0 * identity_hits as f64 / a.len().max(1) as f64
    );
    if a.len() == b.len() {
        let (d, o) = (mean(&diagonal), mean(&off_diagonal));
        eprintln!(
            "mean similarity: diagonal {d:.3} | off-diagonal {o:.3} | separation {:.3}",
            d - o
        );
    }
}

// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: spike <inventory|hash|matrix> ...");
        std::process::exit(2);
    }

    let mut cfg = Config {
        algo: Algo::DHash,
        size: 8,
        edges: false,
        crop: false,
        flip: false,
        scale: 1,
    };
    let mut positional: Vec<String> = Vec::new();
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .unwrap_or_else(|| panic!("{flag} needs a value"))
                .clone()
        };
        match arg.as_str() {
            "--algo" => cfg.algo = Algo::parse(&value("--algo")),
            "--size" => cfg.size = value("--size").parse().expect("size"),
            "--scale" => cfg.scale = value("--scale").parse().expect("scale"),
            "--edges" => cfg.edges = true,
            "--crop" => cfg.crop = true,
            "--flipb" => cfg.flip = true,
            other => positional.push(other.to_string()),
        }
    }
    let paths: Vec<PathBuf> = positional.iter().map(PathBuf::from).collect();

    match args[0].as_str() {
        "inventory" => cmd_inventory(&paths),
        "spreads" => cmd_spreads(&paths[0], &paths[1], &cfg),
        "hash" => cmd_hash(&paths[0], &cfg),
        "matrix" => cmd_matrix(&paths[0], &paths[1..], &cfg),
        other => {
            eprintln!("unknown command {other}");
            std::process::exit(2);
        }
    }
}
