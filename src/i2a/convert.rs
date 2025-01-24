use anyhow::{Context, Result};
use libheif_rs::{HeifContext, LibHeif};
use std::{ffi::OsStr, path::Path};

use super::ConvertRequest;

impl ConvertRequest {
    fn image_extension(&self) -> Result<&OsStr> {
        self.image_path
            .extension()
            .context("No extension found for image path")
    }

    /// convert heic to jpg if necessary
    ///
    /// # Return
    /// true if converted (to output_path)
    ///
    /// # Reference
    /// 1. https://developer.apple.com/documentation/appkit/applying-apple-hdr-effect-to-your-photos
    pub(crate) fn ensure_jpg(&self) -> anyhow::Result<bool> {
        let is_heic = self.image_extension()?.to_ascii_lowercase() == "heic";
        if !is_heic {
            return Ok(false);
        }
        self.convert_heic_to_jpg(&self.image_path, &self.output_path)?;
        debug!(size=%self.output_path.metadata()?.len(), "heic converted to jpg");
        // sync metadata
        self.exif_tool()
            .copy_meta(&self.image_path, &self.output_path)
            .context("write exiftool failed")?;
        trace!("heic convert: jpg exif copied");
        Ok(true)
    }

    /// Return Some(headroom) if HDR heic, None if not HDR heic
    fn get_apple_headroom_from_exif(&self, path: &Path) -> anyhow::Result<Option<f32>> {
        // credit: https://github.com/johncf/apple-hdr-heic/blob/e64716c29abc91a3b40543d7c47fb0f526608982/src/apple_hdr_heic/metadata.py#L17
        // reference: https://developer.apple.com/documentation/appkit/images_and_pdf/applying_apple_hdr_effect_to_your_photos
        //            https://github.com/exiftool/exiftool/blob/405674e0/lib/Image/ExifTool/Apple.pm
        // verify HDRGainMapVersion key
        let hdr_version = self.exif_tool().get_value(path, "xmp:HDRGainMapVersion")?;
        let Some(hdr_version) = hdr_version else {
            debug!("no HDRGainMapVersion, not HDR heic");
            return Ok(None);
        };
        trace!("detected Apple HDRGainMapVersion = {hdr_version}");
        if let Some(headroom) = self.exif_tool().get_value(path, "xmp:HDRGainMapHeadroom")? {
            trace!(%headroom, "got xmp:HDRGainMapHeadroom");
            let headroom = headroom
                .parse::<f32>()
                .context("Invalid HDRGainMapHeadroom value")?;
            return Ok(Some(headroom));
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
        } else if marker48 <= 0.01 {
            -70.0 * marker48 + 3.0
        } else {
            -0.303 * marker48 + 2.303
        };
        let headroom = (2.0_f32).powf(stops.max(0.0));
        Ok(Some(headroom))
    }

    fn get_apple_gainmap_image(
        lib_heif: &libheif_rs::LibHeif,
        handle: &libheif_rs::ImageHandle,
    ) -> anyhow::Result<libheif_rs::Image> {
        for aux_handle in handle.auxiliary_images(libheif_rs::AuxiliaryImagesFilter::new()) {
            // expected "urn:com:apple:photo:2020:aux:hdrgainmap"
            // as per <https://developer.apple.com/documentation/appkit/applying-apple-hdr-effect-to-your-photos>
            let aux_type = aux_handle.auxiliary_type()?;
            trace!("heic-convert: handle type = {aux_type}");
            if aux_type != "urn:com:apple:photo:2020:aux:hdrgainmap" {
                continue;
            }
            let aux_image = lib_heif.decode(
                &aux_handle,
                // libheif_rs::ColorSpace::Rgb(libheif_rs::RgbChroma::Rgb),
                libheif_rs::ColorSpace::Undefined,
                None,
            )?;
            debug!(
                "heic-convert: aux image (gainmap) {} x {}",
                aux_image.width(),
                aux_image.height()
            );
            return Ok(aux_image);
        }
        anyhow::bail!("No auxiliary image found with name urn:com:apple:photo:2020:aux:hdrgainmap")
    }

    /// Returns encoded grayscale image
    fn create_gainmap_jpg(
        apple_hdr_gainmap: &libheif_rs::Image,
        apple_headroom: f32,
    ) -> anyhow::Result<Vec<u8>> {
        let planes = apple_hdr_gainmap.planes();
        let hdr_gainmap = planes.y.context("hdr_gain planes y is None")?;
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
                let recovery = log_recovery.clamp(0.0, 1.0);
                let encoded_recovery = (recovery * 255.0 + 0.5).floor() as u8;
                ultradr_data[i * width + j] = encoded_recovery;
            }
        }

        let jpg_bytes = std::panic::catch_unwind(|| -> std::io::Result<Vec<u8>> {
            let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_GRAYSCALE);
            comp.set_quality(85.0);
            comp.set_size(width, height);
            let mut comp = comp.start_compress(Vec::new())?;
            comp.write_scanlines(&ultradr_data)?;
            let writer = comp.finish()?;
            Ok(writer)
        })
        .map_err(|_| anyhow::anyhow!("mozjpeg failed"))??;
        debug!("re-encoded gainmap jpg size = {}", jpg_bytes.len());

        Ok(jpg_bytes)
    }

    #[inline]
    fn reverse_srgb_transform(u: f32) -> f32 {
        if u <= 0.04045 {
            u / 12.92
        } else {
            ((u + 0.055) / 1.055).powf(2.4)
        }
    }

    fn convert_primary_image_to_jpg(image: &libheif_rs::Image) -> Result<Vec<u8>> {
        let (w, h) = (image.width() as usize, image.height() as usize);

        let colorspace = image.color_space().context("no color space")?;
        anyhow::ensure!(colorspace == libheif_rs::ColorSpace::YCbCr(libheif_rs::Chroma::C420));
        let y_bits = image
            .bits_per_pixel(libheif_rs::Channel::Y)
            .context("no bits per pixel")?;
        anyhow::ensure!(y_bits == 8);
        let planes = image.planes();
        let y = planes.y.context("no y plane")?;
        let cb = planes.cb.context("no cb")?;
        let cr = planes.cr.context("no cr")?;

        anyhow::ensure!(cb.width == w as u32 / 2);
        anyhow::ensure!(cb.height == h as u32 / 2);

        let jpg = std::panic::catch_unwind(|| -> std::io::Result<Vec<u8>> {
            let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_YCbCr);

            comp.set_size(w, h);
            comp.set_quality(95.0);
            let mut comp = comp.start_compress(Vec::new())?;

            // replace with your image data
            let mut scanline = vec![0u8; w * 3];
            for i in 0..h {
                for j in 0..w {
                    let index = i * y.stride + j;
                    scanline[j * 3] = y.data[index];
                    let index = i / 2 * cb.stride + j / 2;
                    scanline[j * 3 + 1] = cb.data[index];
                    scanline[j * 3 + 2] = cr.data[index];
                }
                comp.write_scanlines(&scanline)?;
            }

            let writer = comp.finish()?;
            Ok(writer)
        })
        .map_err(|_| anyhow::anyhow!("mozjpeg failed"))??;
        debug!("re-encoded primary image size = {}", jpg.len());

        Ok(jpg)
    }

    fn convert_heic_to_jpg(&self, src: &Path, output: &Path) -> anyhow::Result<()> {
        let profile = self
            .exif_tool()
            .get_value(src, "ProfileDescription")?
            .context("No profile description found")?;
        trace!(%profile, "ProfileDescription");
        anyhow::ensure!(profile.starts_with("Display P3"));
        // open image and decode
        let lib_heif = LibHeif::new();
        let ctx = HeifContext::read_from_file(src.to_str().unwrap())?;
        let handle = ctx.primary_image_handle()?;
        let (width, height) = (handle.width(), handle.height());
        debug!(width, height, "heic-convert: heic file opened");

        let primary_colorspace = handle.preferred_decoding_colorspace()?;
        trace!("primary colorspace: {:?}", primary_colorspace);
        let primary_image = lib_heif.decode(&handle, primary_colorspace, None)?;
        let mut primary_image = Self::convert_primary_image_to_jpg(&primary_image)?;

        // check if apple HDR
        let apple_headroom = self.get_apple_headroom_from_exif(src)?;
        debug!(?apple_headroom, "apple headroom");
        let Some(apple_headroom) = apple_headroom else {
            debug!("not apple HDR, skip ultra HDR");
            std::fs::write(output, &primary_image)?;
            return Ok(());
        };

        // write ultra HDR image
        let mut encoder = libultrahdr_rs::Encoder::new();
        encoder.set_base_image_quality(90)?;
        encoder.set_gainmap_image_quality(90)?;

        let mut base_image = libultrahdr_rs::CompressedImage::from_bytes(&mut primary_image);
        *base_image.color_gamut_mut() = libultrahdr_rs::sys::uhdr_color_gamut::UHDR_CG_DISPLAY_P3;
        encoder
            .set_compressed_base_image(base_image)
            .context("cannot set base_image")?;

        // get gainmap
        let apple_gainmap = Self::get_apple_gainmap_image(&lib_heif, &handle)?;
        let mut gainmap_jpg = Self::create_gainmap_jpg(&apple_gainmap, apple_headroom)?;
        let gainmap_jpg_compressed = libultrahdr_rs::CompressedImage::from_bytes(&mut gainmap_jpg);
        let metadata = libultrahdr_rs::GainmapMetadata {
            max_content_boost: [apple_headroom; 3],
            min_content_boost: [1.0; 3],
            gamma: [1.0; 3],
            offset_sdr: [0.0; 3],
            offset_hdr: [0.0; 3],
            hdr_capacity_min: 1.0,
            hdr_capacity_max: apple_headroom,
            use_base_cg: 1,
        };
        encoder.set_gainmap_image(gainmap_jpg_compressed, metadata)?;

        encoder.encode().context("encode failed")?;
        let output_img = encoder.get_encoded_stream().context("no encoded stream")?;

        std::fs::write(output, output_img.as_bytes())?;

        Ok(())
    }
}
