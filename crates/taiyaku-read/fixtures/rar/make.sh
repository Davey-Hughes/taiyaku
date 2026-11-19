#!/usr/bin/env bash
# Makes the RAR fixtures and their manifests. No test runs it: it records how
# each fixture was made. Every page with bytes is generated noise or a
# gradient, so no published page is in the repository.
#
#   make.sh <rar 7.23> <rar 6.24> <empty output directory>
#
# rar 7.23 has no -ma switch and cannot write RAR4, so the RAR4 fixtures need
# rar 6.24's -ma4. Everything is written inside the output directory.
set -euo pipefail

[ $# -eq 3 ] || { echo "usage: make.sh <rar 7.23> <rar 6.24> <empty output directory>" >&2; exit 2; }
RAR7=$(realpath "$1")
RAR6=$(realpath "$2")
OUT=$(realpath "$3")
[ -d "$OUT" ] && [ -z "$(ls -A "$OUT")" ] || { echo "not an empty directory: $3" >&2; exit 2; }
# rar reads non-ASCII names from the command line in the locale's encoding.
export LC_ALL=C.UTF-8
SRC=$OUT/src
mkdir "$SRC"

# The manifest of the fixture begun last. Sizes and sha256 come from the
# source files, so they do not depend on any RAR tool reading them back.
begin() { FX=$1; DIR=$SRC/$1; mkdir "$DIR"; MAN=$OUT/$1.manifest; printf 'format\t%s\n' "$2" > "$MAN"; }
sum() { sha256sum < "$DIR/$1" | cut -d' ' -f1; }
entry() {
  local size=0
  [ "$2" = dir ] || size=$(stat -c %s "$DIR/$1")
  printf 'entry\t%s\t%s\t%s\n' "$1" "$2" "$size" >> "$MAN"
}
# `page <name> <reason>`, the reason `encrypted` or `reference`, lists a page in
# reading order whose read fails.
page() { printf 'page\t%s\t%s\n' "$1" "${2:-$(sum "$1")}" >> "$MAN"; }
other() { printf 'other\t%s\t%s\n' "$1" "$(sum "$1")" >> "$MAN"; }
refused() { printf 'refused\tnot a readable RAR archive\n' >> "$MAN"; }
rar7() { (cd "$DIR" && "$RAR7" a -cfg- -idq "$@"); }
rar6() { (cd "$DIR" && "$RAR6" a -cfg- -idq -ma4 "$@"); }

# Pages: noise for the size-bound fixtures, so data dominates headers; an 8x8
# gradient elsewhere, its colour set per page so no two pages share bytes.
noise() { mkdir -p "$(dirname "$DIR/$3")"; magick -seed "$1" -size "$2" xc: +noise Random "PNG24:$DIR/$3"; }
small() { mkdir -p "$(dirname "$DIR/$2")"; magick -size 8x8 "gradient:rgb($1,$((255 - $1)),64)-black" "PNG24:$DIR/$2"; }
text() { mkdir -p "$(dirname "$DIR/$1")"; printf '%s\n' "$2" > "$DIR/$1"; }
COMICINFO='<?xml version="1.0" encoding="utf-8"?>
<ComicInfo><Title>Fixture</Title><Number>1</Number></ComicInfo>'

begin rar5-stored.rar rar5
for i in 1 2 3 4 5 6; do noise "$i" 180x180 "p0$i.png"; done
rar7 -m0 "$OUT/$FX" p01.png p02.png p03.png p04.png p05.png p06.png
for i in 1 2 3 4 5 6; do entry "p0$i.png" file; done
for i in 1 2 3 4 5 6; do page "p0$i.png"; done

begin rar5-compressed.rar rar5
small 10 p10.png; text ComicInfo.xml "$COMICINFO"; small 20 p2.png; small 30 p1.png
rar7 "$OUT/$FX" p10.png ComicInfo.xml p2.png p1.png
entry p10.png file; entry ComicInfo.xml file; entry p2.png file; entry p1.png file
page p1.png; page p2.png; page p10.png
other ComicInfo.xml

begin rar5-solid.rar rar5
small 40 p1.png; small 50 p2.png; small 60 p3.png; small 70 p4.png
rar7 -s "$OUT/$FX" p1.png p2.png p3.png p4.png
entry p1.png file; entry p2.png file; entry p3.png file; entry p4.png file
page p1.png; page p2.png; page p3.png; page p4.png

# Three runs, so only the middle entry carries a password.
begin rar5-encrypted-entry.rar rar5
small 80 p01.png; small 90 p02.png; small 100 p03.png
rar7 "$OUT/$FX" p01.png
rar7 -psecret "$OUT/$FX" p02.png
rar7 "$OUT/$FX" p03.png
entry p01.png file; entry p02.png encrypted; entry p03.png file
page p01.png; page p02.png encrypted; page p03.png

# Headers encrypted: nothing can be listed without the password.
begin rar5-headers-encrypted.rar rar5
small 110 p01.png
rar7 -hpsecret "$OUT/$FX" p01.png
refused

# rar always writes directory records after the files, so vol/cover.png and vol
# come last. A named directory is recursed in the file system's own order, so
# the files are named one by one and vol is added last without its contents.
# Naming __MACOSX's one file, not the directory, keeps its directory records out.
begin rar5-layout.rar rar5
small 120 vol/p01.png; small 130 vol/p02.png; small 140 vol/.hidden.png
mkdir "$DIR/vol/cover.png"
mkdir -p "$DIR/__MACOSX/vol"; printf '\0\5\26\7\0\2\0\0' > "$DIR/__MACOSX/vol/._p01.png"
printf '\0\0\0\1Bud1' > "$DIR/.DS_Store"
rar7 "$OUT/$FX" vol/p01.png vol/p02.png vol/.hidden.png __MACOSX/vol/._p01.png .DS_Store
rar7 "$OUT/$FX" vol/cover.png
rar7 '-xvol/*' "$OUT/$FX" vol
entry vol/p01.png file; entry vol/p02.png file; entry vol/.hidden.png file
entry __MACOSX/vol/._p01.png file; entry .DS_Store file
entry vol/cover.png dir; entry vol dir
page vol/p01.png; page vol/p02.png

begin rar5-japanese.rar rar5
small 150 第01話/001.png; small 160 第01話/002.png; small 170 第02話/001.png; small 180 表紙.png
rar7 "$OUT/$FX" 第01話/001.png 第01話/002.png 第02話/001.png 表紙.png
entry 第01話/001.png file; entry 第01話/002.png file; entry 第02話/001.png file; entry 表紙.png file
page 第01話/001.png; page 第01話/002.png; page 第02話/001.png; page 表紙.png

begin rar5-no-pages.rar rar5
text ComicInfo.xml "$COMICINFO"; text notes.txt 'No page images here.'
rar7 "$OUT/$FX" ComicInfo.xml notes.txt
entry ComicInfo.xml file; entry notes.txt file
other ComicInfo.xml

# -oi1:1 stores the second of two identical files, however small, as a
# reference to the first. cp, not a second magick run, keeps them identical.
begin rar5-file-reference.rar rar5
small 220 p01.png; cp "$DIR/p01.png" "$DIR/p02.png"
rar7 -oi1:1 "$OUT/$FX" p01.png p02.png
entry p01.png file; entry p02.png reference
page p01.png; page p02.png reference

begin rar4.rar rar4
for i in 1 2 3 4; do noise "$((10 + i))" 148x148 "p0$i.png"; done
noise 15 148x148 ページ05.png
rar6 -m0 "$OUT/$FX" p01.png p02.png p03.png p04.png ページ05.png
for i in 1 2 3 4; do entry "p0$i.png" file; done; entry ページ05.png file
for i in 1 2 3 4; do page "p0$i.png"; done; page ページ05.png

begin rar4-encrypted-entry.rar rar4
small 190 p01.png; small 200 p02.png; small 210 p03.png
rar6 "$OUT/$FX" p01.png
rar6 -psecret "$OUT/$FX" p02.png
rar6 "$OUT/$FX" p03.png
entry p01.png file; entry p02.png encrypted; entry p03.png file
page p01.png; page p02.png encrypted; page p03.png

# libarchive reads no solid RAR4: it refuses the archive at its second entry.
begin rar4-solid.rar rar4
small 230 p01.png; small 240 p02.png; small 250 p03.png
rar6 -s "$OUT/$FX" p01.png p02.png p03.png
entry p01.png file; entry p02.png file; entry p03.png file
refused

# Stored with a BLAKE2 hash in place of the CRC32 taiyaku checks.
begin rar5-blake2.rar rar5
noise 31 64x64 p01.png; noise 32 64x64 p02.png
rar7 -m0 -htb "$OUT/$FX" p01.png p02.png
entry p01.png file; entry p02.png file
page p01.png; page p02.png

begin rar5-empty.rar rar5
small 245 p01.png; : > "$DIR/p02.png"
rar7 "$OUT/$FX" p01.png p02.png
entry p01.png file; entry p02.png empty
page p01.png; page p02.png empty

begin rar4-empty.rar rar4
small 235 p01.png; : > "$DIR/p02.png"
rar6 "$OUT/$FX" p01.png p02.png
entry p01.png file; entry p02.png empty
page p01.png; page p02.png empty

# The first two volumes of a set of three, each read alone. A volume holds a
# whole page and the part of a page that goes on into the next volume.
begin rar4-split.part1.rar rar4
for i in 1 2 3 4; do noise "$((40 + i))" 64x64 "p0$i.png"; done
PAGES=$DIR
rar6 -m0 -v20k "$OUT/rar4-split.rar" p01.png p02.png p03.png p04.png
rm "$OUT/rar4-split.part3.rar"
entry p01.png file; entry p02.png split
page p01.png; page p02.png split

begin rar4-split.part2.rar rar4
cp "$PAGES"/p0*.png "$DIR"
entry p02.png split; entry p03.png file; entry p04.png split
page p02.png split; page p03.png; page p04.png split

begin rar5-split.part1.rar rar5
for i in 1 2 3 4; do noise "$((50 + i))" 64x64 "p0$i.png"; done
rar7 -m0 -v20k "$OUT/rar5-split.rar" p01.png p02.png p03.png p04.png
rm "$OUT"/rar5-split.part[2-9].rar
entry p01.png file; entry p02.png split
refused

# The last volume of a set of two: it opens with the end of a page begun in the
# first volume, which libarchive does not list, and holds two whole pages.
begin rar5-split-last.part2.rar rar5
for i in 1 2 3; do noise "$((60 + i))" 64x64 "p0$i.png"; done
noise 64 32x32 p04.png
rar7 -m0 -v24k "$OUT/rar5-split-last.rar" p01.png p02.png p03.png p04.png
rm "$OUT/rar5-split-last.part1.rar"
entry p03.png file; entry p04.png file
page p03.png; page p04.png

# A comment and a recovery record are service blocks with data of their own.
begin rar4-comment.rar rar4
small 15 p01.png; small 25 p02.png; text comment.txt 'A comment for the walk to step over.'
rar6 -zcomment.txt "$OUT/$FX" p01.png p02.png
entry p01.png file; entry p02.png file
page p01.png; page p02.png

begin rar4-recovery.rar rar4
small 35 p01.png; small 45 p02.png
rar6 -rr5p "$OUT/$FX" p01.png p02.png
entry p01.png file; entry p02.png file
page p01.png; page p02.png
