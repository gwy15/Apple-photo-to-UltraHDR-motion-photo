use anyhow::{Context, Result};
use libheif_rs::{HeifContext, LibHeif};
use std::{ffi::OsStr, path::Path};
use tempfile::NamedTempFile;

use super::ConvertRequest;

impl ConvertRequest {
    fn image_extension(&self) -> Result<&OsStr> {
        self.image_path
            .extension()
            .context("No extension found for image path")
    }

    /// convert heic to jpg if necessary
    ///
    /// # Reference
    /// 1. https://developer.apple.com/documentation/appkit/applying-apple-hdr-effect-to-your-photos
    pub(crate) fn ensure_jpg(&self) -> anyhow::Result<Option<NamedTempFile>> {
        let is_heic = self.image_extension()?.to_ascii_lowercase() == "heic";
        if !is_heic {
            return Ok(None);
        }
        let tempfile = self.convert_heic_to_jpg(&self.image_path)?;
        Ok(Some(tempfile))
    }

    fn get_apple_headroom_from_exif(&self, path: &Path) -> anyhow::Result<f32> {
        // credit: https://github.com/johncf/apple-hdr-heic/blob/e64716c29abc91a3b40543d7c47fb0f526608982/src/apple_hdr_heic/metadata.py#L17
        // reference: https://developer.apple.com/documentation/appkit/images_and_pdf/applying_apple_hdr_effect_to_your_photos
        //            https://github.com/exiftool/exiftool/blob/405674e0/lib/Image/ExifTool/Apple.pm
        // verify HDRGainMapVersion key
        self.exif_tool()
            .get_value(path, "xmp:HDRGainMapVersion")?
            .inspect(|v| debug!("HDRGainMapVersion = {v}"))
            .context("No HDRGainMapVersion found, not HDR heic")?;
        if let Some(headroom) = self.exif_tool().get_value(path, "xmp:HDRGainMapHeadroom")? {
            debug!(%headroom, "got xmp:HDRGainMapHeadroom");
            return headroom
                .parse::<f32>()
                .context("Invalid HDRGainMapHeadroom value");
        }
        // get markers
        let marker33 = self
            .exif_tool()
            .get_value(path, "MakerNotes:HDRHeadroom")?
            .context("No Markers MakerNotes:HDRHeadroom found, not HDR heic")?
            .parse::<f32>()
            .context("Invalid Marker33 value")?;
        let marker48 = self
            .exif_tool()
            .get_value(path, "MakerNotes:HDRGain")?
            .context("No Markers MakerNotes:HDRGain found, not HDR heic")?
            .parse::<f32>()
            .context("Invalid Marker48 value")?;
        let stops = if marker33 < 1.0 {
            if marker48 <= 0.01 {
                -20.0 * marker48 + 1.8
            } else {
                -0.101 * marker48 + 1.601
            }
        } else {
            if marker48 <= 0.01 {
                -70.0 * marker48 + 3.0
            } else {
                -0.303 * marker48 + 2.303
            }
        };
        let headroom = (2.0_f32).powf(stops.max(0.0));
        Ok(headroom)
    }

    fn get_apple_gainmap_image(
        lib_heif: &libheif_rs::LibHeif,
        handle: &libheif_rs::ImageHandle,
    ) -> anyhow::Result<libheif_rs::Image> {
        let aux_ids = handle.get_auxiliary_image_ids();
        for aux_id in aux_ids {
            let aux_handle = handle.get_auxiliary_image_handle(aux_id)?;
            // expected "urn:com:apple:photo:2020:aux:hdrgainmap"
            // as per <https://developer.apple.com/documentation/appkit/applying-apple-hdr-effect-to-your-photos>
            let aux_type = aux_handle.get_auxiliary_type()?;
            debug!(
                "heic-convert: [{aux_id}]: handle = {aux_handle:?}, type = {:?}",
                aux_type.to_string_lossy()
            );
            if aux_type.as_bytes() != b"urn:com:apple:photo:2020:aux:hdrgainmap" {
                continue;
            }
            let aux_image = lib_heif.decode(
                &aux_handle,
                // libheif_rs::ColorSpace::Rgb(libheif_rs::RgbChroma::Rgb),
                libheif_rs::ColorSpace::Undefined,
                None,
            )?;
            debug!(
                "heic-convert: [{aux_id}]: aux image: {:?} ({} x {})",
                aux_image,
                aux_image.width(),
                aux_image.height()
            );
            return Ok(aux_image);
        }
        anyhow::bail!("No auxiliary image found with name urn:com:apple:photo:2020:aux:hdrgainmap")
    }

    /// Returns encoded grayscale image
    fn create_ultra_hdr_gainmap(
        apple_hdr_gainmap: &libheif_rs::Image,
        apple_headroom: f32,
    ) -> anyhow::Result<Vec<u8>> {
        let planes = apple_hdr_gainmap.planes();
        debug!("planes: {planes:?}");
        let hdr_gainmap = planes.y.context("hdr_gain planes y is None")?;
        debug!("plane: size={}", hdr_gainmap.data.len());
        let (width, height) = (hdr_gainmap.width as usize, hdr_gainmap.height as usize);
        anyhow::ensure!(hdr_gainmap.storage_bits_per_pixel == 8);
        anyhow::ensure!(hdr_gainmap.data.len() == hdr_gainmap.stride * height);

        let mut ultradr_data = vec![0u8; width * height];
        let log_hr = apple_headroom.ln();
        let hr_1 = apple_headroom - 1.0;
        for i in 0..height {
            for j in 0..width {
                let index = i * hdr_gainmap.stride + j;
                let u = hdr_gainmap.data[index] as f32 / 255.0;
                // transform encoded hdr gainmap to linear value by applying reverse sRGB transform
                let hdr_gainmap_linear = Self::reverse_srgb_transform(u);
                // As per <https://developer.android.com/media/platform/hdr-image-format#encode>,
                // we choose offset_hdr = offset_sdr = 0,
                // min_content_boost = 1.0, max_content_boost = apple_headroom
                // Thus we have log_recovery = log(apple_headroom, pixel_gain)
                //                           = log(hr, 1 + (hr - 1) * gainmap_linear)
                let log_recovery = (1.0 + hr_1 * hdr_gainmap_linear).ln() / log_hr;
                // we choose map_gamma = 1.0
                let recovery = log_recovery.max(0.0).min(1.0);
                let encoded_recovery = (recovery * 255.0 + 0.5).floor() as u8;
                ultradr_data[i * width as usize + j] = encoded_recovery;
            }
        }
        Ok(ultradr_data)
    }

    #[inline]
    fn reverse_srgb_transform(u: f32) -> f32 {
        if u <= 0.04045 {
            u / 12.92
        } else {
            ((u + 0.055) / 1.055).powf(2.4)
        }
    }

    fn convert_image_libheif_to_libultrahdr(
        image: &mut libheif_rs::Image,
    ) -> anyhow::Result<libultrahdr_rs::RawImage<'_>> {
        // TODO: YCbCr => Linear Y
        use libultrahdr_rs::sys;
        let (width, height) = (image.width(), image.height());
        // color space to fmt
        let colorspace = image.color_space().context("No colorspace in heic")?;
        debug!("colorspace: {:?}", colorspace);
        let fmt = match colorspace {
            libheif_rs::ColorSpace::Undefined => anyhow::bail!("No colorspace in heic (undefined)"),
            libheif_rs::ColorSpace::YCbCr(chroma) => match chroma {
                libheif_rs::Chroma::C420 => sys::uhdr_img_fmt::UHDR_IMG_FMT_12bppYCbCr420,
                libheif_rs::Chroma::C422 => sys::uhdr_img_fmt::UHDR_IMG_FMT_16bppYCbCr422,
                libheif_rs::Chroma::C444 => match image.bits_per_pixel(libheif_rs::Channel::Y) {
                    Some(8) => sys::uhdr_img_fmt::UHDR_IMG_FMT_24bppYCbCr444,
                    Some(10) => sys::uhdr_img_fmt::UHDR_IMG_FMT_30bppYCbCr444,
                    p => anyhow::bail!("Unknown YCbCr with {p:?} bits per pixel"),
                },
            },
            libheif_rs::ColorSpace::Rgb(rgb_chroma) => {
                anyhow::bail!("unsupported image color space ({rgb_chroma:?})")
            }
            libheif_rs::ColorSpace::Monochrome => {
                anyhow::bail!("unsupported image color space (monochrome)")
            }
        };
        debug!("uhdr converted format: {colorspace:?} => {fmt:?}");

        let heif_planes = image.planes_mut();
        let mut uhdr_planes = [&mut [] as &mut [u8], &mut [], &mut []];
        let mut stride = [0; 3];
        match fmt {
            sys::uhdr_img_fmt::UHDR_IMG_FMT_12bppYCbCr420 => {
                let (y, cb, cr) = (
                    heif_planes.y.unwrap(),
                    heif_planes.cb.unwrap(),
                    heif_planes.cr.unwrap(),
                );
                stride[sys::UHDR_PLANE_Y as usize] = y.stride as u32;
                stride[sys::UHDR_PLANE_U as usize] = cb.stride as u32;
                stride[sys::UHDR_PLANE_V as usize] = cr.stride as u32;
                uhdr_planes[sys::UHDR_PLANE_Y as usize] = y.data;
                uhdr_planes[sys::UHDR_PLANE_U as usize] = cb.data;
                uhdr_planes[sys::UHDR_PLANE_V as usize] = cr.data;
            }
            _ => anyhow::bail!("unsupported image format"),
        };

        let uhdr_raw = libultrahdr_rs::RawImage {
            fmt,
            color_gamut: sys::uhdr_color_gamut::UHDR_CG_BT_709,
            color_transfer: sys::uhdr_color_transfer::UHDR_CT_SRGB,
            range: sys::uhdr_color_range::UHDR_CR_FULL_RANGE,
            width,
            height,
            planes: uhdr_planes,
            stride: stride,
        };
        Ok(uhdr_raw)
    }

    fn convert_heic_to_jpg(&self, path: &Path) -> anyhow::Result<NamedTempFile> {
        let apple_headroom = self.get_apple_headroom_from_exif(path)?;
        debug!(%apple_headroom, "apple headroom");

        let lib_heif = LibHeif::new();
        let ctx = HeifContext::read_from_file(path.to_str().unwrap())?;
        let handle = ctx.primary_image_handle()?;
        debug!(width=%handle.width(), height=%handle.height(), "heic-convert: file opened");

        // decode
        let primary_colorspace = handle.preferred_decoding_colorspace()?;
        debug!("primary colorspace: {:?}", primary_colorspace);
        let mut primary_image = lib_heif.decode(&handle, primary_colorspace, None)?;

        let hdr_gainmap = Self::get_apple_gainmap_image(&lib_heif, &handle)?;
        let ultrahdr_gainmap_rgb = Self::create_ultra_hdr_gainmap(&hdr_gainmap, apple_headroom)?;

        // encode
        let mut encoder = libultrahdr_rs::Encoder::new();
        let raw_sdr_image = Self::convert_image_libheif_to_libultrahdr(&mut primary_image)?;
        encoder.set_raw_sdr_image(raw_sdr_image)?;
        // let primary_image = Self::convert_image_libheif_to_libultrahdr(&mut primary_image)?;
        // encoder.set_raw_hdr_image(primary_image)?;
        // encoder.

        // encoder.set_gainmap_image(img, metadata)?;

        encoder.encode().context("encode failed")?;
        let output = encoder.get_encoded_stream().context("no encoded stream")?;
        std::fs::write("testoutput/ultrahdr_sdr.jpg", output.as_bytes())?;

        // let mut context = HeifContext::new()?;
        // let mut encoder = lib_heif.encoder_for_format(libheif_rs::CompressionFormat::Hevc)?;
        // encoder.set_quality(libheif_rs::EncoderQuality::Lossy(70))?;
        // context.encode_image(&hdr_gain, &mut encoder, None)?;
        // context.write_to_file("testoutput/aux.heic")?;

        // TODO: make jpg
        todo!()
    }
}
