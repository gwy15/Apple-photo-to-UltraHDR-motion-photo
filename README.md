# Apple-photo-to-UltraHDR-motion-photo

[![Release](https://github.com/gwy15/Apple-photo-to-UltraHDR-motion-photo/actions/workflows/release.yml/badge.svg)](https://github.com/gwy15/Apple-photo-to-UltraHDR-motion-photo/actions/workflows/release.yml)

Converts Apple's photos to ultra HDR photos, keeping HDR effect and motion photo (live photo).

将苹果照片（heic+mov）转为 jpg 格式，保留 HDR 效果和动态照片效果。

## Build

Depends on several packages:
- libclang (compile time only)
- libheif
- libultrahdr -> libjpeg (dummy for libturbojpeg)
- libturbojpeg
- libffmpeg (LGPL)

### Build on Linux
See workflow file.

### Build on Windows
vcpkg and VS required.

See workflow file.

### Build on Mac
See workflow file.

## Usage
```
Usage: aa-photo-bridge.exe [OPTIONS] --original <ORIGINAL> <PATH>

Arguments:
  <PATH>  Path to the directory containing images and videos to convert

Options:
  -e, --exiftool <EXIFTOOL>
          Path to the exiftool executable
  -j, --parallel
          Run in parallel mode
  -o, --original <ORIGINAL>
          What to do with the original files [possible values: keep, delete]
      --image-extensions <IMAGE_EXTENSIONS>
          Image extensions. Default: "heic,jpg,jpeg"
      --video-extensions <VIDEO_EXTENSIONS>
          Video extensions. Default: "mov,mp4"
  -q, --image-quality <IMAGE_QUALITY>
          Image quality. Default: 85 [default: 85]
  -g, --gainmap-quality <GAINMAP_QUALITY>
          Gainmap quality. Default: 85 [default: 85]
      --strict
          Strict mode: exit on multiple images / videos with same name
  -v, --verbose
          Print more detailed runtime information
  -h, --help
          Print help
```

## Example
```bash
aa-photo-bridge d:\tmp\iPhone -o delete -e 'c:\Program Files\exiftool\exiftool.exe' -j --strict
# i.e.,
aa-photo-bridge d:\tmp\iPhone --original delete --exiftool 'c:\Program Files\exiftool\exiftool.exe' -j --strict
```

## Known problems
- [ ] Some videos are internally marked with a "rotate" flag. Video players handle them correctly, but photo albums may not. In that case, I recommend use `scripts/preprocess-fix-rotations.py` and do a ffmpeg re-encode before converting.
- [ ] Internet downloaded photo files may have wrong creation time / modification time. In that case, I recommend use `scripts/postprocess-set-file-times.py` which sets file ctime/mtime as photo time in exif if present.
- [x] Audio in motion photos does not work, at least on my Xiaomi phone. This is because Apple encodes audio in pcm_s16le, which is not widely supported.
    - [x] TODO: use ffmpeg-cli or libffmpeg to convert audio to aac / ac3.
