#!/usr/bin/env bash
# Builds a static, LGPL-only ffmpeg + ffprobe for one macOS target, with libass (burned-in
# captions), freetype, fribidi and harfbuzz, and Apple's VideoToolbox / AudioToolbox encoders.
# No GPL parts (no libx264): configure runs without --enable-gpl.
#
#   scripts/build-ffmpeg.sh aarch64-apple-darwin
#   scripts/build-ffmpeg.sh x86_64-apple-darwin      (cross-compiles on Apple Silicon)
#
# Output: src-tauri/binaries/ffmpeg-<target>, ffprobe-<target>
# Build-time tools only: meson, ninja, nasm (brew install meson ninja nasm). Nothing from them ships.
set -euo pipefail

TARGET="${1:?usage: build-ffmpeg.sh <aarch64-apple-darwin|x86_64-apple-darwin>}"
case "$TARGET" in
  aarch64-apple-darwin) ARCH=arm64; CPU=aarch64 ;;
  x86_64-apple-darwin) ARCH=x86_64; CPU=x86_64 ;;
  *) echo "unsupported target $TARGET (Windows and Linux: see THIRD_PARTY_NOTICES.md)"; exit 1 ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CACHE="$ROOT/.sidecar-cache"
SRC="$CACHE/src"
WORK="$CACHE/build-$TARGET"
PREFIX="$WORK/prefix"
OUT="$ROOT/src-tauri/binaries"
MINOS=12.3
JOBS="$(sysctl -n hw.ncpu)"

# name|url|sha256  (pinned; bump together with THIRD_PARTY_NOTICES.md)
SOURCES=(
  "ffmpeg-8.0.1|https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz|05ee0b03119b45c0bdb4df654b96802e909e0a752f72e4fe3794f487229e5a41"
  "freetype-2.14.1|https://download.savannah.gnu.org/releases/freetype/freetype-2.14.1.tar.xz|32427e8c471ac095853212a37aef816c60b42052d4d9e48230bab3bdf2936ccc"
  "fribidi-1.0.16|https://github.com/fribidi/fribidi/releases/download/v1.0.16/fribidi-1.0.16.tar.xz|1b1cde5b235d40479e91be2f0e88a309e3214c8ab470ec8a2744d82a5a9ea05c"
  "harfbuzz-12.1.0|https://github.com/harfbuzz/harfbuzz/releases/download/12.1.0/harfbuzz-12.1.0.tar.xz|e5c81b7f6e0b102dfb000cfa424538b8e896ab78a2f4b8a5ec8cae62ab43369e"
  "libass-0.17.4|https://github.com/libass/libass/releases/download/0.17.4/libass-0.17.4.tar.xz|78f1179b838d025e9c26e8fef33f8092f65611444ffa1bfc0cfac6a33511a05a"
)

mkdir -p "$SRC" "$WORK" "$PREFIX" "$OUT"
for entry in "${SOURCES[@]}"; do
  IFS='|' read -r name url sha <<<"$entry"
  file="$SRC/$(basename "$url")"
  [ -f "$file" ] || curl -sSfL -o "$file" "$url"
  echo "$sha  $file" | shasum -a 256 -c --quiet - || { echo "checksum mismatch for $name"; rm -f "$file"; exit 1; }
  rm -rf "$WORK/$name" && tar -xf "$file" -C "$WORK"
done

export CFLAGS="-arch $ARCH -mmacosx-version-min=$MINOS -O2"
export CXXFLAGS="$CFLAGS"
export LDFLAGS="-arch $ARCH -mmacosx-version-min=$MINOS"
# Only our own prefix: Homebrew's .pc files must never be found.
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
unset PKG_CONFIG_PATH

CROSS="$WORK/cross.ini"
cat >"$CROSS" <<EOF
[binaries]
c = 'clang'
cpp = 'clang++'
objc = 'clang'
ar = 'ar'
strip = 'strip'
pkg-config = 'pkg-config'
[built-in options]
c_args = ['-arch', '$ARCH', '-mmacosx-version-min=$MINOS']
cpp_args = ['-arch', '$ARCH', '-mmacosx-version-min=$MINOS']
objc_args = ['-arch', '$ARCH', '-mmacosx-version-min=$MINOS']
c_link_args = ['-arch', '$ARCH', '-mmacosx-version-min=$MINOS']
cpp_link_args = ['-arch', '$ARCH', '-mmacosx-version-min=$MINOS']
[host_machine]
system = 'darwin'
cpu_family = '$CPU'
cpu = '$CPU'
endian = 'little'
EOF

meson_build() {
  local dir="$1"; shift
  (cd "$WORK/$dir" && meson setup build --cross-file "$CROSS" --prefix "$PREFIX" --libdir lib \
    --default-library=static --buildtype=release --wrap-mode=nodownload "$@" >/dev/null &&
    ninja -C build -j "$JOBS" >/dev/null && ninja -C build install >/dev/null)
  echo "built $dir"
}

meson_build freetype-2.14.1 -Dbrotli=disabled -Dharfbuzz=disabled -Dpng=disabled -Dzlib=disabled -Dbzip2=disabled -Dtests=disabled
meson_build fribidi-1.0.16 -Ddocs=false -Dbin=false -Dtests=false
meson_build harfbuzz-12.1.0 -Dglib=disabled -Dgobject=disabled -Dcairo=disabled -Dchafa=disabled -Dicu=disabled \
  -Dfreetype=enabled -Dtests=disabled -Ddocs=disabled -Dutilities=disabled -Dbenchmark=disabled -Dintrospection=disabled
meson_build libass-0.17.4 -Dfontconfig=disabled -Dcoretext=enabled -Ddirectwrite=disabled -Dasm=disabled \
  -Dlibunibreak=disabled -Dtest=disabled -Dcompare=disabled -Dprofile=disabled -Dfuzz=disabled -Dcheckasm=disabled

cd "$WORK/ffmpeg-8.0.1"
CROSS_FLAGS=()
[ "$ARCH" = "$(uname -m)" ] || CROSS_FLAGS=(--enable-cross-compile --arch="$CPU" --target-os=darwin)
./configure --prefix="$PREFIX" ${CROSS_FLAGS[@]+"${CROSS_FLAGS[@]}"} \
  --cc="clang -arch $ARCH" --cxx="clang++ -arch $ARCH" \
  --extra-cflags="-mmacosx-version-min=$MINOS -I$PREFIX/include" \
  --extra-ldflags="-mmacosx-version-min=$MINOS -L$PREFIX/lib" \
  --extra-libs="-lc++" \
  --pkg-config-flags=--static \
  --enable-static --disable-shared --disable-debug --disable-doc --disable-ffplay \
  --disable-autodetect --disable-x86asm \
  --enable-avfoundation --enable-videotoolbox --enable-audiotoolbox --enable-coreimage \
  --enable-zlib --enable-bzlib --enable-iconv \
  --enable-libass --enable-libfreetype --enable-libfribidi --enable-libharfbuzz >/dev/null
grep -q "^#define CONFIG_GPL 0" config.h || { echo "refusing: build would be GPL"; exit 1; }
make -j "$JOBS" >/dev/null 2>&1 || make -j "$JOBS"
strip ffmpeg ffprobe
cp ffmpeg "$OUT/ffmpeg-$TARGET"
cp ffprobe "$OUT/ffprobe-$TARGET"
echo "ffmpeg ready: $OUT/ffmpeg-$TARGET ($(du -h "$OUT/ffmpeg-$TARGET" | cut -f1))"
otool -L "$OUT/ffmpeg-$TARGET" | grep -E "/opt/homebrew|/usr/local" && { echo "links outside the system"; exit 1; } || true
