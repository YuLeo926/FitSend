# FitSend's Windows FFmpeg build

This recipe builds separate Windows x64 `ffmpeg.exe` and `ffprobe.exe` tools.
It replaces the broad third-party Essentials distribution with a pinned build
whose FFmpeg and non-system dependency sources can be shipped together.
It is not a claim of byte-for-byte reproducibility or a legal opinion.

## Inputs and scope

- FFmpeg 8.0.1, revision `894da5ca7d742e4429ffb2af534fcda0103ef593`.
- x264 `0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee` (GPL encoder).
- libvpx 1.15.2, Opus 1.5.2, zlib 1.3.2, dav1d 1.5.1.
- Exact URLs and SHA-256 hashes: `sources.lock`. FFmpeg's unmodified GitHub
  source archive is mirrored in FitSend's existing release assets.
- Static libraries, Windows-native threading, no network protocols, no
  automatic detection of additional host libraries, no nonfree components.
- H.264/AAC output, VP8/VP9/Opus support, AV1 decoding, PNG handling, and
  FFmpeg's built-in codecs/filters, including the SSIM verification filter.
- No patches to upstream source. Configure/CMake/Meson generate ordinary
  build files; all commands and generated configuration are preserved.

## Build (Ubuntu 24.04)

Install build tools (not required on end users' Windows computers):

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends build-essential \
  gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 nasm pkg-config \
  cmake meson ninja-build curl unzip ca-certificates
bash scripts/ffmpeg/build-windows.sh /absolute/path/to/a-new-output-directory
```

The output directory must not exist. A build records installed package versions,
compiler details, configuration, upstream licenses and PE imports in `materials`.
Windows system libraries and the standard MinGW/GCC toolchain are prerequisites;
their distribution notices are included. The GCC runtime uses its upstream
runtime exception; it is not an extra media dependency downloaded by this recipe.

## Rebuild from the distributed source bundle

Extract `FitSend-FFmpeg-8.0.1-fitsend1-sources.tar.gz`. It contains the **actual
checksummed source archives used by this build**, not just download pointers.
With the build tools above installed:

```sh
bash materials/recipe/build-windows.sh /absolute/path/to/new-build "$PWD/sources"
```

The second argument disables source downloads and verifies the supplied archives.
The recorded toolchain versions help reconstruct the environment, but this does
not promise identical executable hashes with a different compiler/runner image.

## Release gate

Download the two build artifacts. Extract the binary package and run the app's
Windows core/integration/real-media tests with those exact tools. Stage it using
`FITSEND_FFMPEG_SOURCE_DIR`, then rebuild installers and portable ZIP. Publish
the corresponding source bundle beside those downloads and include it in release
checksums. Do not substitute binaries from another build while reusing this bundle.
The binaries are GPL v3-or-later; dependency notices accompany every app package.
See [FFmpeg's legal information](https://ffmpeg.org/legal.html).
