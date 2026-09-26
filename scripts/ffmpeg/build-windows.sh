#!/usr/bin/env bash
# Build only into a newly created directory. Never cleans an existing checkout.
set -euo pipefail
recipe=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
out=${1:?Usage: build-windows.sh NEW_OUTPUT_DIRECTORY [SOURCE_ARCHIVES_DIRECTORY]}
mkdir -- "$out"
out=$(cd -- "$out" && pwd)
archive_cache=${2:-}
jobs=${JOBS:-$(nproc)}
mkdir -p "$out"/{sources,src,prefix,package/bin,materials/recipe,materials/licenses,materials/config}
cp "$recipe"/{build-windows.sh,mingw.ini,sources.lock,README.md} "$out/materials/recipe/"
exec > >(tee "$out/materials/build.log") 2>&1
prefix="$out/prefix"
export CC=x86_64-w64-mingw32-gcc-win32 CXX=x86_64-w64-mingw32-g++-win32
export AR=x86_64-w64-mingw32-ar RANLIB=x86_64-w64-mingw32-ranlib
export PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig" PKG_CONFIG_PATH=
export LC_ALL=C
export SOURCE_DATE_EPOCH=1762732800

while IFS='|' read -r name archive hash url; do
    [[ -z "$name" || "$name" == \#* ]] && continue
    if [[ -n "$archive_cache" ]]; then
        cp -- "$archive_cache/$archive" "$out/sources/$archive"
    else
        curl --fail --location --retry 3 --connect-timeout 30 --max-time 600 "$url" -o "$out/sources/$archive"
    fi
    printf '%s  %s\n' "$hash" "$out/sources/$archive" | sha256sum --check --strict
    mkdir "$out/src/$name"
    if [[ "$archive" == *.zip ]]; then
        unzip -q "$out/sources/$archive" -d "$out/src/$name"
        shopt -s dotglob
        contents=("$out/src/$name"/*)
        [[ ${#contents[@]} == 1 && -d "${contents[0]}" ]]
        mv "${contents[0]}"/* "$out/src/$name/"
        rmdir "${contents[0]}"
        shopt -u dotglob
    else
        tar -xf "$out/sources/$archive" --strip-components=1 -C "$out/src/$name"
    fi
done < "$recipe/sources.lock"

{
    uname -a
    cat /etc/os-release
    "$CC" -v 2>&1
    dpkg-query -W
} > "$out/materials/toolchain.txt"

# No host libraries can leak in through pkg-config. Every non-system library is
# built here from a checksummed archive and linked statically.
mkdir -p "$prefix"/{lib/pkgconfig,include}
cd "$out/src/zlib"
make -j"$jobs" -f win32/Makefile.gcc PREFIX=x86_64-w64-mingw32- CC="$CC" libz.a
cp libz.a "$prefix/lib/"
cp zlib.h zconf.h "$prefix/include/"
printf 'prefix=%s\nlibdir=${prefix}/lib\nincludedir=${prefix}/include\nName: zlib\nDescription: zlib compression library\nVersion: 1.3.2\nLibs: -L${libdir} -lz\nCflags: -I${includedir}\n' "$prefix" > "$prefix/lib/pkgconfig/zlib.pc"

cd "$out/src/x264"
./configure --host=x86_64-w64-mingw32 --cross-prefix=x86_64-w64-mingw32- \
    --prefix="$prefix" --enable-static --disable-cli --disable-opencl
make -j"$jobs"
make install
cp config.mak "$out/materials/config/x264-config.mak"

cd "$out/src/libvpx"
CROSS=x86_64-w64-mingw32- ./configure --target=x86_64-win64-gcc --prefix="$prefix" \
    --disable-examples --disable-tools --disable-docs --disable-unit-tests \
    --enable-vp9-highbitdepth --enable-static --disable-shared
make -j"$jobs"
make install
cp config.mk "$out/materials/config/libvpx-config.mk"

cmake -S "$out/src/opus" -B "$out/src/opus/build" \
    -DCMAKE_SYSTEM_NAME=Windows -DCMAKE_C_COMPILER="$CC" \
    -DCMAKE_RC_COMPILER=x86_64-w64-mingw32-windres \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$prefix" \
    -DCMAKE_INSTALL_LIBDIR=lib -DBUILD_SHARED_LIBS=OFF \
    -DOPUS_BUILD_PROGRAMS=OFF -DOPUS_BUILD_TESTING=OFF \
    -DOPUS_INSTALL_PKG_CONFIG_MODULE=ON
cmake --build "$out/src/opus/build" --parallel "$jobs"
cmake --install "$out/src/opus/build"
cp "$out/src/opus/build/CMakeCache.txt" "$out/materials/config/opus-CMakeCache.txt"

meson setup "$out/src/dav1d/build" "$out/src/dav1d" \
    --cross-file "$recipe/mingw.ini" --prefix "$prefix" --libdir lib \
    --buildtype release --default-library static --wrap-mode nodownload \
    -Denable_tools=false -Denable_tests=false -Denable_examples=false \
    -Denable_docs=false -Dxxhash_muxer=disabled
meson compile -C "$out/src/dav1d/build" -j "$jobs"
meson install -C "$out/src/dav1d/build"
cp "$out/src/dav1d/build/meson-info/intro-buildoptions.json" "$out/materials/config/dav1d-options.json"

cd "$out/src/ffmpeg"
./configure --prefix="$out/ffmpeg-install" --target-os=mingw32 --arch=x86_64 \
    --enable-cross-compile --cross-prefix=x86_64-w64-mingw32- --cc="$CC" --cxx="$CXX" \
    --pkg-config=pkg-config --pkg-config-flags=--static \
    --disable-autodetect --disable-network --disable-ffplay --disable-doc --disable-debug \
    --disable-shared --enable-static --enable-w32threads --disable-pthreads \
    --enable-gpl --enable-version3 --enable-libx264 --enable-libvpx --enable-libopus \
    --enable-libdav1d --enable-zlib --extra-version=fitsend1 \
    --extra-cflags="-I$prefix/include" --extra-ldflags="-L$prefix/lib -static -static-libgcc -static-libstdc++"
make -j"$jobs"
make install
cp "$out/ffmpeg-install/bin/"{ffmpeg.exe,ffprobe.exe} "$out/package/bin/"
cp ffbuild/config.mak config.h "$out/materials/config/"
cp COPYING.GPLv3 "$out/package/LICENSE"

# Preserve upstream notices, including patent grants where present. Full sources
# are also distributed, not replaced by links or license summaries.
for name in ffmpeg x264 libvpx opus zlib dav1d; do
    mkdir "$out/materials/licenses/$name"
    find "$out/src/$name" -maxdepth 1 -type f \
        \( -iname 'COPYING*' -o -iname 'LICENSE*' -o -iname 'PATENTS*' -o -iname 'AUTHORS*' \) \
        -exec cp '{}' "$out/materials/licenses/$name/" \;
done
cp "$out/src/zlib/zlib.h" "$out/materials/licenses/zlib/"
mkdir "$out/materials/licenses/toolchain"
for package in mingw-w64-common mingw-w64-x86-64-dev gcc-mingw-w64-base gcc-mingw-w64-x86-64-win32 gcc-mingw-w64-x86-64-win32-runtime; do
    cp "/usr/share/doc/$package/copyright" "$out/materials/licenses/toolchain/$package.txt"
done
for exe in ffmpeg ffprobe; do
    x86_64-w64-mingw32-objdump -p "$out/package/bin/$exe.exe" > "$out/materials/$exe-pe.txt"
    # No undeclared runtime DLLs: only the Windows system libraries below.
    imports=$(awk '/DLL Name:/{print tolower($3)}' "$out/materials/$exe-pe.txt")
    [[ -n "$imports" ]]
    while read -r dll; do
        case "$dll" in
            kernel32.dll|msvcrt.dll|user32.dll|advapi32.dll|shell32.dll|ole32.dll|oleaut32.dll|ws2_32.dll|bcrypt.dll|secur32.dll|gdi32.dll|winmm.dll|psapi.dll) ;;
            *) echo "Unreviewed DLL import: $dll"; exit 1 ;;
        esac
    done <<< "$imports"
done
{
    printf 'FitSend FFmpeg 8.0.1-fitsend1, Windows x86_64, GPL v3 or later.\n'
    printf 'Built from the unmodified source archives identified below.\n'
    printf 'Configuration and exact toolchain package versions are in the accompanying source bundle.\n'
    printf 'No network protocols, nonfree components, or dynamically linked third-party DLLs.\n'
    printf 'Corresponding sources and rebuild recipe: FitSend-FFmpeg-8.0.1-fitsend1-sources.tar.gz\n'
    printf 'Available alongside FitSend downloads: https://github.com/YuLeo926/FitSend/releases\n\n'
    cat "$recipe/sources.lock"
} > "$out/package/README.txt"
{
    for license in "$out/materials/licenses"/*/*; do
        printf '\n===== %s =====\n\n' "${license#"$out/materials/licenses/"}"
        cat "$license"
    done
} > "$out/package/THIRD_PARTY_LICENSES.txt"
cp "$recipe/sources.lock" "$out/package/SOURCES.lock"
cp "$out/materials/toolchain.txt" "$out/package/TOOLCHAIN.txt"
cd "$out/package"
sha256sum bin/*.exe LICENSE README.txt THIRD_PARTY_LICENSES.txt SOURCES.lock TOOLCHAIN.txt > SHA256SUMS.txt
cp SHA256SUMS.txt "$out/materials/binary-SHA256SUMS.txt"
# Flush/close the live build log before archiving the materials.
echo 'Compilation, import audit, and package checksums completed.'
exec 1>&- 2>&-
wait
cd "$out"
tar -czf FitSend-FFmpeg-8.0.1-fitsend1-sources.tar.gz sources materials
tar -czf FitSend-FFmpeg-8.0.1-fitsend1-windows-x64.tar.gz package
