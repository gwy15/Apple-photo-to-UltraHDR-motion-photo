# Apple-photo-to-UltraHDR-motion-photo

Converts Apple's photos to ultra HDR photos, keeping HDR effect and motion photo (live photo).

将苹果照片（heic+mov）转为 jpg 格式，保留 HDR 效果和动态照片效果。

## Build

Depends on several packages:
- libclang (compile time only)
- libheif
- libultrahdr -> libjpeg (dummy for libturbojpeg)
- libturbojpeg

### Build on Linux
See `scripts/ghw-build.sh` or workflow file.

### Build on Windows
vcpkg and VS required.

See `scripts/windows-build.ps1` or workflow file.

### Build on Mac
See `scripts/ghw-build.sh` or workflow file.

## Usage
```
Usage: main.exe [OPTIONS] --original <ORIGINAL> <PATH>

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
main d:\tmp\iPhone -o delete -e 'c:\Program Files\exiftool\exiftool.exe' -j --strict
# i.e.,
main d:\tmp\iPhone --original delete --exiftool 'c:\Program Files\exiftool\exiftool.exe' -j --strict
```

## Known problems
- [ ] Audio in motion photos does not work, at least on my Xiaomi phone.
