# Apple-photo-to-UltraHDR-motion-photo

Converts Apple's photos to ultra HDR photos, keeping HDR effect and motion photo (live photo).

将苹果照片（heic+mov）转为 jpg 格式，保留 HDR 效果和动态照片效果。

## Build

Depends on several packages:
- libclang (compile time only)
- libheif
- libultrahdr -> libjpeg (dummy for libturbojpeg)
- libturbojpeg
