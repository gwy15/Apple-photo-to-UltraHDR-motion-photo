uhdr_codec_private_t* handle = uhdr_create_encoder();

uhdr_enc_set_raw_image(handle, &mRawRgba1010102Image, UHDR_HDR_IMG);
uhdr_enc_set_compressed_image(handle, &mSdrIntentCompressedImage,
    (mGainMapCompressedFile != nullptr &&
     mGainMapMetadataCfgFile != nullptr)
        ? UHDR_BASE_IMG
        : UHDR_SDR_IMG)
uhdr_enc_set_gainmap_image(handle, &mGainMapCompressedImage,
                                              &mGainMapMetadata)

// configs
uhdr_enc_set_quality(handle, mQuality, UHDR_BASE_IMG)
uhdr_enc_set_gainmap_scale_factor(handle, mMapDimensionScaleFactor)
uhdr_enc_set_min_max_content_boost(handle, mMinContentBoost,
                                                      mMaxContentBoost)

// encode
uhdr_encode(handle)
auto output = uhdr_get_encoded_stream(handle);
uhdr_release_encoder(handle)
