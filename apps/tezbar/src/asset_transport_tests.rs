use super::*;
use ::core::prelude::v1::test;
use image::ImageFormat;

fn encoded(format: ImageFormat, w: u32, h: u32) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(w, h, image::Rgb([240, 10, 20])))
        .write_to(&mut bytes, format)
        .unwrap();
    bytes.into_inner()
}
#[test]
fn decoder_accepts_actual_sdk_rasters_and_bounds_variants() {
    for format in [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::WebP,
        ImageFormat::Gif,
        ImageFormat::Tiff,
    ] {
        let bytes = encoded(format, 1024, 16);
        let result = decode_asset(&bytes, 128).unwrap();
        assert!(result.image.size(0).width.0 <= 128);
        assert!(result.decoded_bytes <= ASSET_BYTES);
    }
    assert!(decode_asset(&encoded(ImageFormat::Bmp, 1, 1), 128).is_err());
    assert!(decode_asset(&encoded(ImageFormat::Png, 1, 1), 981).is_err());
    assert!(decode_asset(&vec![0; ASSET_BYTES + 1], 128).is_err());
}
#[test]
fn decoder_checks_header_allocation_before_upload_and_keeps_m5_strict() {
    let bytes = encoded(ImageFormat::Png, 16385, 1);
    assert!(decode_asset(&bytes, 128).is_err());
    assert!(decode_chat_preview(&bytes).is_none());
    let bytes = encoded(ImageFormat::Png, 16384, 1);
    assert!(decode_chat_preview(&bytes).is_some());
    let bytes = encoded(ImageFormat::Png, 2049, 1);
    assert!(tezbar_ui::image_cache::decode_png(&bytes).is_err());
    assert!(decode_asset(&bytes, 128).is_ok());
    let result = decode_asset(&encoded(ImageFormat::Png, 1, 1), 128).unwrap();
    assert_eq!(result.image.as_bytes(0).unwrap(), [20, 10, 240, 255]);
}
#[test]
fn svg_uses_locked_renderer_with_no_file_or_nested_data_resolver() {
    let bytes = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100000" height="100000"><rect width="100000" height="100000" fill="red"/><image href="file:///must-not-be-read" width="100000" height="100000"/></svg>"#;
    let result = decode_asset(bytes, 128).unwrap();
    assert_eq!(result.image.size(0).width.0, 128);
    assert_eq!(&result.image.as_bytes(0).unwrap()[..4], [0, 0, 255, 255]);
    assert_eq!(result.decoded_bytes, 128 * 128 * 4);
}
#[test]
fn chat_source_budget_is_independent_of_asset_preview_budget() {
    let mut bytes = encoded(ImageFormat::Png, 1, 1);
    bytes.resize(CHAT_SOURCE_BYTES, 0);
    let payload = chat_image_payload(bytes.clone(), Some("chosen.png".into())).unwrap();
    assert_eq!(
        STANDARD.decode(payload["data"].as_str().unwrap()).unwrap(),
        bytes
    );
    assert_eq!(payload["mimeType"], "image/png");
    assert_eq!(payload["width"], 1);
    assert!(decode_asset(&bytes, 128).is_err());
    bytes.push(0);
    assert!(chat_image_payload(bytes, None).is_err());
    assert!(chat_image_payload(encoded(ImageFormat::Gif, 1, 1), None).is_err());
}

fn core(root: &std::path::Path) -> CoreClient {
    let paths = tezbar_core::common::CorePaths {
        config_dir: root.join("config"),
        data_dir: root.join("data"),
        home_dir: root.join("home"),
        temp_dir: root.join("tmp"),
        resource_dir: root.join("resources"),
    };
    let context = tezbar_core::common::CoreContext::new(
        paths,
        Some(Arc::new(tezbar_core::host::TestHost::default())),
    );
    CoreClient::new(tezbar_core::NativeCore::new(context).unwrap())
}
#[tokio::test]
async fn real_client_denies_forged_scope_and_effect_mismatch_without_host_or_network() {
    let root = tempfile::tempdir().unwrap();
    let client = core(root.path());
    let path = root.path().join("private.png");
    std::fs::write(&path, encoded(ImageFormat::Png, 1, 1)).unwrap();
    let scope = AssetScope {
        extension_root: root.path().canonicalize().unwrap(),
        allowed_legacy: [path.clone()].into(),
    };
    let result = client
        .load_variant(
            scope,
            Source::File(path.clone()),
            Effect::ReadFile {
                token: 1,
                path,
                max_bytes: ASSET_BYTES,
            },
            128,
        )
        .await;
    assert!(result.is_err());
    let result = client
        .load_catalog_variant(
            Source::Data("wrong".into()),
            Effect::DecodeData {
                token: 2,
                data: "data:image/png;base64,AQ==".into(),
                max_bytes: ASSET_BYTES,
            },
            128,
        )
        .await;
    assert!(result.is_err());
}
#[tokio::test]
async fn real_client_uses_registry_identity_and_same_core_service() {
    let root = tempfile::tempdir().unwrap();
    let package = root.path().join("data/extensions/asset-fixture");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(
        package.join("package.json"),
        br#"{"title":"Assets","commands":[]}"#,
    )
    .unwrap();
    let path = package.join("image.png");
    std::fs::write(&path, encoded(ImageFormat::Png, 1, 1)).unwrap();
    let client = core(root.path());
    let scope = client.scope("raycast.asset-fixture".into()).await.unwrap();
    assert_eq!(scope.extension_root, package.canonicalize().unwrap());
    let result = client
        .load_variant(
            scope.clone(),
            Source::File(path.clone()),
            Effect::ReadFile {
                token: 1,
                path,
                max_bytes: ASSET_BYTES,
            },
            128,
        )
        .await
        .unwrap();
    assert_eq!(result.decoded_bytes, 4);
    let mut forged = scope;
    forged.allowed_legacy.insert(root.path().join("other"));
    assert!(
        client
            .load_variant(
                forged,
                Source::File(package.join("image.png")),
                Effect::ReadFile {
                    token: 2,
                    path: package.join("image.png"),
                    max_bytes: ASSET_BYTES
                },
                128
            )
            .await
            .is_err()
    );
}

#[test]
fn malformed_raster_svg_and_zero_variant_never_upload() {
    for bytes in [
        b"invalid".as_slice(),
        b"\x89PNG\r\n\x1a\n",
        b"<svg broken>",
        b"GIF89a",
    ] {
        assert!(decode_asset(bytes, 128).is_err());
    }
    assert!(decode_asset(&encoded(ImageFormat::Png, 1, 1), 0).is_err());
}
