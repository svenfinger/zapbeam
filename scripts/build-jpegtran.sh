#!/bin/sh
set -eu
# Native builds only; invoke separately on each distribution target.
root=$(CDPATH='' cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fL https://github.com/mozilla/mozjpeg/archive/refs/tags/v4.1.5.tar.gz -o "$work/source.tar.gz"
printf '%s  %s\n' 9fcbb7171f6ac383f5b391175d6fb3acde5e64c4c4727274eade84ed0998fcc1 "$work/source.tar.gz" | shasum -a 256 -c -
tar -xzf "$work/source.tar.gz" -C "$work"
cmake -S "$work/mozjpeg-4.1.5" -B "$work/build" -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DBUILD=20240930 -DCMAKE_BUILD_TYPE=Release -DENABLE_SHARED=OFF -DENABLE_STATIC=ON -DWITH_TURBOJPEG=OFF -DPNG_SUPPORTED=OFF -DWITH_SIMD=OFF
cmake --build "$work/build" --target jpegtran-static -j 2
target=$(rustc -vV | sed -n 's/^host: //p')
mkdir -p "$root/src-tauri/binaries"
cp "$work/build/jpegtran-static" "$root/src-tauri/binaries/jpegtran-$target"
