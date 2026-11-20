#!/usr/bin/env bash
# Writes the JPEG XL fixtures the decoder tests compare with djxl.
# Every input is drawn here from fixed seeds, so no published page is kept
# in the repository and anyone can regenerate the set. Each X.djxl.png is
# libjxl's own decode of X.jxl (frame 1 for animated.jxl): the reference a
# decode is held to. The checks at the end fail the run when a fixture does
# not show what its tests rely on.
# Usage: make.sh [out-dir]   (default: the directory holding this script)
# Needs cjxl, djxl and jxlinfo (libjxl 0.12), magick (ImageMagick 7), cjpeg,
# brotli, xxd, ffmpeg (ImageMagick's own APNG writer changes pixels) and oxipng
# (recompresses PNGs without touching a pixel, keeping the set under 200 KB).
set -euo pipefail

out=${1:-$(dirname -- "$0")}
tmp=$(mktemp -d)
trap 'rm -rf -- "${tmp:?}"' EXIT

# Undated, unprofiled PNGs: the same inputs give the same pixels every run,
# and cjxl sees no metadata that would make it write a container.
png=(-strip -define png:exclude-chunk=date,time)

# A 96x128 page: a gradient, thin ink lines and a strip of low-amplitude
# noise, so a lossy codec has edges, flats and texture to get wrong. The
# noise is confined to a strip to keep the committed set small.
page() { # seed, output
  magick -size 96x128 gradient:'#fbf3e4-#46587a' \
    -fill none -stroke black -strokewidth 1 \
    -draw 'line 6,8 89,8' -draw 'line 6,14 89,40' -draw 'line 24,4 24,100' \
    -draw 'circle 62,64 62,84' \
    \( -size 96x20 xc:gray50 -seed "$1" +noise Random +level 40%,60% \) \
    -gravity south -geometry +0+4 -composite \
    "${png[@]}" PNG24:"$2"
}

enc() { cjxl --quiet --num_threads=0 "$@"; }
ref() { djxl --quiet --num_threads=0 "$out/$1.jxl" "$out/$1.djxl.png"; }
fail() { echo "make.sh: $*" >&2; exit 1; }
same() { local r; r=$(magick compare -metric AE "$1" "$2" null: 2>&1 || true); [ "${r%% *}" = 0 ]; }

page 1 "$tmp/lossy.png"
enc -d 1 "$tmp/lossy.png" "$out/lossy.jxl"

page 2 "$out/lossless.png"
enc -d 0 "$out/lossless.png" "$out/lossless.jxl"

page 3 "$tmp/grey-rgb.png"
magick "$tmp/grey-rgb.png" -colorspace Gray -depth 8 "${png[@]}" \
  -define png:color-type=0 PNG:"$tmp/grey.png"
enc -d 0 "$tmp/grey.png" "$out/grey.jxl"

# Top-left quarter fully transparent with black RGB under it, a band at half
# alpha, the rest opaque: a decoder that drops or mishandles alpha shows it.
page 4 "$tmp/alpha-rgb.png"
magick "$tmp/alpha-rgb.png" -fill black -draw 'rectangle 0,0 47,63' \
  \( -size 96x128 xc:white -fill black -draw 'rectangle 0,0 47,63' \
     -fill gray50 -draw 'rectangle 48,0 95,15' \) \
  -alpha off -compose CopyOpacity -composite "${png[@]}" PNG32:"$tmp/alpha.png"
enc -d 0 "$tmp/alpha.png" "$out/alpha.jxl"

page 5 "$out/animated.frame1.png"
magick "$out/animated.frame1.png" -rotate 180 "${png[@]}" PNG24:"$out/animated.frame2.png"
cp "$out/animated.frame1.png" "$tmp/frame1.png"
cp "$out/animated.frame2.png" "$tmp/frame2.png"
ffmpeg -v error -y -framerate 2 -start_number 1 -i "$tmp/frame%d.png" \
  -pix_fmt rgb24 -plays 0 -f apng "$tmp/animated.apng"
enc -d 0 "$tmp/animated.apng" "$out/animated.jxl"

magick -size 2048x2048 xc:'#8a5b3c' "${png[@]}" PNG24:"$tmp/large.png"
enc -d 0 "$tmp/large.png" "$out/large.jxl"

page 6 "$tmp/recompressed.png"
magick "$tmp/recompressed.png" PPM:"$tmp/recompressed.ppm"
cjpeg -quality 85 -outfile "$out/recompressed.jpg" "$tmp/recompressed.ppm"
enc --lossless_jpeg=1 "$out/recompressed.jpg" "$out/recompressed.jxl"

# A JPEG with bytes after its end marker, as some cameras and apps leave
# them; cjxl keeps them, so the rebuild ends on them and not on EOI.
page 7 "$tmp/trailing.png"
magick "$tmp/trailing.png" PPM:"$tmp/trailing.ppm"
cjpeg -quality 85 -outfile "$tmp/trailing-bare.jpg" "$tmp/trailing.ppm"
{ cat "$tmp/trailing-bare.jpg"; printf 'bytes after the end marker'; } >"$out/trailing.jpg"
enc --lossless_jpeg=1 "$out/trailing.jpg" "$out/trailing.jxl"

# A page carrying libjxl's own Display P3 profile, embedded in its PNG (so not
# "${png[@]}", whose -strip would drop it). Lossless: cjxl keeps a profile
# whole on a lossless page and turns it into an enum encoding on a lossy one.
page 8 "$tmp/icc-page.png"
enc -d 0 -x color_space=RGB_D65_DCI_Rel_SRG "$tmp/icc-page.png" "$tmp/p3.jxl"
djxl --quiet "$tmp/p3.jxl" "$tmp/p3.png" --icc_out="$tmp/p3.icc"
magick "$tmp/icc-page.png" -profile "$tmp/p3.icc" -define png:exclude-chunk=date,time PNG24:"$tmp/icc.png"
enc -d 0 "$tmp/icc.png" "$out/icc.jxl"

# A metadata bomb: recompressed.jxl with a brob box of Exif after ftyp, whose
# short brotli stream inflates to 64 MiB of zeros, far past the decoder's cap.
# Small enough that a test watched without the cap still runs.
head -c 67108864 /dev/zero | brotli -q 5 -c >"$tmp/bomb.br"
bomb_box=$((12 + $(stat -c %s "$tmp/bomb.br")))
{
  head -c 32 "$out/recompressed.jxl"
  printf '%08x' "$bomb_box" | xxd -r -p
  printf 'brobExif'
  cat "$tmp/bomb.br"
  tail -c +33 "$out/recompressed.jxl"
} >"$out/bomb.jxl"

# Metadata near the cap, for the tests that pin where it lies: brob boxes of
# zeros (inner type, then inflated size), on recompressed.jxl, which a rebuild
# needs whole, or on lossy.jxl's codestream boxed with no jbrd, so the boxes
# are all the metadata the page has.
zeros_box() { # inner type, inflated bytes
  head -c "$2" /dev/zero | brotli -q 5 -c >"$tmp/zeros.br"
  [ "$(brotli -dc "$tmp/zeros.br" | wc -c)" -eq "$2" ] || fail "a box of $2 zeros does not inflate to $2"
  printf '%08x' $((12 + $(stat -c %s "$tmp/zeros.br"))) | xxd -r -p
  printf 'brob%s' "$1"
  cat "$tmp/zeros.br"
}
on_recompressed() { head -c 32 "$out/recompressed.jxl"; cat; tail -c +33 "$out/recompressed.jxl"; }
on_lossy() {
  head -c 32 "$out/recompressed.jxl"
  cat
  printf '%08x' $((8 + $(stat -c %s "$out/lossy.jxl"))) | xxd -r -p
  printf jxlc
  cat "$out/lossy.jxl"
}
{ zeros_box Exif 4194304; zeros_box 'xml ' 4194304; } | on_recompressed >"$out/metadata-two-4mib.jxl"
zeros_box Exif 16777216 | on_lossy >"$out/metadata-at-cap.jxl"
zeros_box Exif 16777217 | on_lossy >"$out/metadata-past-cap.jxl"
{ zeros_box Exif 9437184; zeros_box 'xml ' 9437184; } | on_lossy >"$out/metadata-two-9mib.jxl"

for f in lossy lossless grey alpha large recompressed icc; do ref "$f"; done
# djxl writes every frame of an animation to a PNG; a PPM takes the first.
djxl --quiet --num_threads=0 "$out/animated.jxl" "$tmp/animated.ppm"
magick "$tmp/animated.ppm" "${png[@]}" PNG24:"$out/animated.djxl.png"
oxipng --quiet --nx --opt max --strip safe "$out"/*.png

# The facts the tests lean on.
[ "$(head -c 20 "$out/recompressed.jxl" | tail -c 8 | od -An -tx1 | tr -d ' \n')" = 0000001466747970 ] ||
  fail "recompressed.jxl's ftyp box is not the 20 bytes after the signature, where bomb.jxl's box goes"
[ "$(brotli -dc "$tmp/bomb.br" | wc -c)" -eq 67108864 ] || fail "bomb.jxl's box does not inflate to 64 MiB"
[ "$(head -c 2 "$out/lossy.jxl" | od -An -tx1 | tr -d ' \n')" = ff0a ] ||
  fail "lossy.jxl is not a bare codestream"
[ "$(head -c 2 "$out/icc.jxl" | od -An -tx1 | tr -d ' \n')" = ff0a ] ||
  fail "icc.jxl is not a bare codestream"
[ "$(head -c 12 "$out/recompressed.jxl" | od -An -tx1 | tr -d ' \n')" = 0000000c4a584c200d0a870a ] ||
  fail "recompressed.jxl is not a container"
djxl --quiet "$out/recompressed.jxl" "$tmp/rebuilt.jpg"
cmp -s "$tmp/rebuilt.jpg" "$out/recompressed.jpg" || fail "recompressed.jxl does not rebuild recompressed.jpg"
[ "$(tail -c 2 "$out/trailing.jpg" | od -An -tx1 | tr -d ' \n')" != ffd9 ] ||
  fail "trailing.jpg ends on its end marker"
djxl --quiet "$out/trailing.jxl" "$tmp/trailing-rebuilt.jpg"
cmp -s "$tmp/trailing-rebuilt.jpg" "$out/trailing.jpg" || fail "trailing.jxl does not rebuild trailing.jpg, tail and all"
jxlinfo "$out/grey.jxl" | grep -q Grayscale || fail "grey.jxl is not one channel"
jxlinfo "$out/icc.jxl" | grep -q 'ICC profile' || fail "icc.jxl carries no ICC profile"
same "$out/lossless.djxl.png" "$out/lossless.png" || fail "lossless.jxl is not lossless"
same "$out/grey.djxl.png" "$tmp/grey.png" || fail "grey.jxl is not lossless"
same "$out/alpha.djxl.png" "$tmp/alpha.png" || fail "alpha.jxl is not lossless"
same "$out/large.djxl.png" "$tmp/large.png" || fail "large.jxl is not lossless"
same "$out/icc.djxl.png" "$tmp/icc.png" || fail "icc.jxl is not lossless"
[ "$(magick "$out/alpha.djxl.png" -crop 48x64+0+0 +repage -channel RGBA -separate -format '%[max] ' info:)" = "0 0 0 0 " ] ||
  fail "alpha.djxl.png's transparent quarter is not black at alpha 0"
djxl --quiet --num_threads=0 --output_frames "$out/animated.jxl" "$tmp/frame.ppm"
same "$tmp/frame.ppm-0.ppm" "$out/animated.frame1.png" || fail "animated.jxl's frame 1 is not animated.frame1.png"
same "$tmp/frame.ppm-1.ppm" "$out/animated.frame2.png" || fail "animated.jxl's frame 2 is not animated.frame2.png"
[ ! -e "$tmp/frame.ppm-2.ppm" ] || fail "animated.jxl has more than two frames"
same "$out/animated.djxl.png" "$out/animated.frame1.png" || fail "animated.djxl.png is not frame 1"
! same "$out/animated.frame1.png" "$out/animated.frame2.png" || fail "the animation's frames do not differ"
