#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Builds FFmpeg's command line tools for the Android package (EXT-001, D-85):
# ffmpeg and ffprobe with LAME, Opus and libwebp, for arm64-v8a and x86_64.
# libmpv-android's FFmpeg plays videos but has no encoders, muxers, filters
# or programs, so cards and embedded subtitles need these.
#
#   packaging/android/ffmpeg.sh <sources> <out>
#
# <sources> holds the four tarballs below (checked against their SHA-256);
# <out> receives <abi>/libffmpeg.so, <abi>/libffprobe.so (the executables,
# named so that Android extracts them beside the app's libraries, where they
# may be run) and <abi>/lib*_nikomiru.so (FFmpeg's libraries, renamed so they
# do not meet libmpv-android's). `cargo xtask android --ffmpeg <out>` puts
# them in the APK. Needs ANDROID_NDK_HOME, make and pkg-config.

set -euo pipefail

FFMPEG=ffmpeg-8.1.3.tar.xz
FFMPEG_SHA=7138d28c96d9d3e3af4ee3d8cad72741f8ffb40da90c1112235dea3ecd3178a3
LAME=lame-3.100.tar.gz
LAME_SHA=ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e
OPUS=opus-1.6.1.tar.gz
OPUS_SHA=6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1
WEBP=libwebp-1.6.0.tar.gz
WEBP_SHA=e4ab7009bf0629fd11982d4c2aa83964cf244cffba7347ecd39019a9e38c4564

API=28
SUFFIX=_nikomiru

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <sources> <out>" >&2
    exit 2
fi
SRC=$(realpath "$1")
mkdir -p "$2"
OUT=$(realpath "$2")
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME}"
BIN=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin
JOBS=${JOBS:-$(nproc)}

check() {
    echo "$2  $SRC/$1" | sha256sum -c --quiet - || {
        echo "$1: checksum mismatch" >&2
        exit 1
    }
}
check "$FFMPEG" "$FFMPEG_SHA"
check "$LAME" "$LAME_SHA"
check "$OPUS" "$OPUS_SHA"
check "$WEBP" "$WEBP_SHA"

WORK=$OUT/.build
rm -rf "$WORK"
mkdir -p "$WORK"

build() {
    local abi=$1 triple=$2 arch=$3
    local cc=$BIN/$triple$API-clang
    local prefix=$WORK/$abi/prefix
    local dir=$WORK/$abi
    mkdir -p "$dir" "$prefix"
    # 16 KiB pages (Android 15), as for the app's own libraries
    local ldflags="-Wl,-z,max-page-size=16384"
    local env=(CC="$cc" AR="$BIN/llvm-ar" RANLIB="$BIN/llvm-ranlib"
        STRIP="$BIN/llvm-strip" NM="$BIN/llvm-nm" CFLAGS="-O2 -fPIC"
        LDFLAGS="$ldflags")

    tar -xf "$SRC/$LAME" -C "$dir"
    (cd "$dir/lame-3.100" &&
        env "${env[@]}" ./configure --host="$triple" --prefix="$prefix" \
            --disable-shared --enable-static --disable-frontend \
            --disable-decoder --with-pic >/dev/null &&
        make -j"$JOBS" >/dev/null && make install >/dev/null)

    tar -xf "$SRC/$OPUS" -C "$dir"
    (cd "$dir/${OPUS%.tar.gz}" &&
        env "${env[@]}" ./configure --host="$triple" --prefix="$prefix" \
            --disable-shared --enable-static --disable-doc \
            --disable-extra-programs --with-pic >/dev/null &&
        make -j"$JOBS" >/dev/null && make install >/dev/null)

    tar -xf "$SRC/$WEBP" -C "$dir"
    (cd "$dir/${WEBP%.tar.gz}" &&
        env "${env[@]}" ./configure --host="$triple" --prefix="$prefix" \
            --disable-shared --enable-static --with-pic \
            --enable-libwebpmux --disable-gl --disable-sdl --disable-png \
            --disable-jpeg --disable-tiff --disable-gif --disable-wic >/dev/null &&
        make -j"$JOBS" >/dev/null && make install >/dev/null)

    tar -xf "$SRC/$FFMPEG" -C "$dir"
    local asm=()
    # x86 assembly needs nasm; without it, the inline assembly left on its
    # own scales pictures wrongly (swscale), so none at all: the x86_64
    # build is for emulators and Chromebooks
    [[ $arch == x86_64 ]] && ! command -v nasm >/dev/null &&
        asm=(--disable-x86asm --disable-inline-asm)
    (cd "$dir/${FFMPEG%.tar.xz}" &&
        PKG_CONFIG_LIBDIR=$prefix/lib/pkgconfig PKG_CONFIG_PATH= \
            ./configure --prefix="$prefix" --target-os=android --arch="$arch" \
            --enable-cross-compile --cc="$cc" --cxx="$cc++" \
            --nm="$BIN/llvm-nm" --ar="$BIN/llvm-ar" --ranlib="$BIN/llvm-ranlib" \
            --strip="$BIN/llvm-strip" --pkg-config=pkg-config \
            --pkg-config-flags=--static \
            --enable-gpl --enable-version3 --enable-shared --disable-static \
            --build-suffix="$SUFFIX" --enable-pic --disable-doc \
            --disable-ffplay --disable-avdevice --disable-debug \
            --enable-libmp3lame --enable-libopus --enable-libwebp \
            --extra-cflags="-I$prefix/include" \
            --extra-ldflags="-L$prefix/lib $ldflags" \
            "${asm[@]}" >"$dir/configure.log" &&
        # `ffmpeg -version` shows the configuration: no folder of this
        # computer in it
        sed -i "s|$prefix|/nikomiru|g; s|$BIN/||g; s|$HOME|~|g" config.h &&
        # the programs find the libraries beside them (make turns \$$ORIGIN
        # into \$ORIGIN, which the shell passes on as $ORIGIN)
        echo 'LDEXEFLAGS += -Wl,-rpath,\$$ORIGIN' >>ffbuild/config.mak &&
        make -j"$JOBS" >/dev/null && make install >/dev/null)

    mkdir -p "$OUT/$abi"
    rm -f "$OUT/$abi"/*.so
    cp "$prefix/bin/ffmpeg" "$OUT/$abi/libffmpeg.so"
    cp "$prefix/bin/ffprobe" "$OUT/$abi/libffprobe.so"
    cp "$prefix"/lib/lib*"$SUFFIX".so "$OUT/$abi/"
    "$BIN/llvm-strip" --strip-unneeded "$OUT/$abi"/*.so
}

build arm64-v8a aarch64-linux-android aarch64
build x86_64 x86_64-linux-android x86_64

{
    echo "FFmpeg for Nikomiru's Android package, built by packaging/android/ffmpeg.sh:"
    for f in "$FFMPEG" "$LAME" "$OPUS" "$WEBP"; do
        echo "  $f  $(sha256sum "$SRC/$f" | cut -d' ' -f1)"
    done
    echo "from https://ffmpeg.org/releases/, https://lame.sourceforge.io/,"
    echo "https://downloads.xiph.org/releases/opus/ and"
    echo "https://storage.googleapis.com/downloads.webmproject.org/releases/webp/."
} >"$OUT/SOURCES.txt"
rm -rf "$WORK"
echo "done: $OUT"
