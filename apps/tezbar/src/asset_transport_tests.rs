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

#[test]
fn repair_svg_reference_expansion_is_bounded_before_usvg_and_icons_render() {
    let mut s = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:x="http://www.w3.org/1999/xlink" width="20" height="20"><defs><g id="n0"><path d="M0 0h1v1z"/></g>"##,
    );
    for n in 1..20 {
        s.push_str(&format!(
            r##"<g id="n{n}"><use x:href="#n{}"/><use href="#n{}"/></g>"##,
            n - 1,
            n - 1
        ));
    }
    s.push_str(r##"</defs><!-- <use fake comment --> <use href="#n19"/></svg>"##);
    let start = std::time::Instant::now();
    assert!(super::decode_chat_preview(s.as_bytes()).is_none());
    assert!(start.elapsed() < std::time::Duration::from_millis(50));
    assert!(super::decode_chat_preview(br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><defs><path id="a" d="M0 0h8v8z"/></defs><use href="#a" fill="red"/></svg>"##).is_some());
}

#[test]
fn review_n1_marker_expansion_is_refused_before_usvg() {
    let vertices = "L1 1 ".repeat(60_000);
    let children = "<path d='M0 0h1v1z'/>".repeat(50);
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><defs><marker id='m'>{children}</marker></defs><path d='M0 0 {vertices}' marker-mid='url(#m)'/></svg>"
    );
    let start = std::time::Instant::now();
    // Check preflight first: the unfixed test never allocates the expanded marker tree.
    assert!(svg_preflight(svg.as_bytes()).is_err());
    assert!(decode_chat_preview(svg.as_bytes()).is_none());
    assert!(start.elapsed() < std::time::Duration::from_millis(50));
}
#[test]
fn review_n1_css_filter_routes_and_excess_layers_are_refused() {
    for body in [
        "<rect width='8' height='8' filter='blur(4)'/>",
        "<rect width='8' height='8' style='filter:drop-shadow(1px 1px 1px black)'/>",
        "<style>rect{filter:blur(4px)}</style><rect width='8' height='8'/>",
    ] {
        let svg =
            format!("<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>{body}</svg>");
        assert!(decode_chat_preview(svg.as_bytes()).is_none(), "{body}");
    }
    let layers = "<g opacity='.5'><rect width='8' height='8'/></g>".repeat(65);
    let svg =
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'>{layers}</svg>");
    assert!(decode_chat_preview(svg.as_bytes()).is_none());
}

#[cfg(any(
    target_os = "macos",
    all(target_os = "windows", target_arch = "x86_64")
))]
#[tokio::test]
async fn review_n4_legacy_registry_icon_reaches_real_client_for_both_spellings() {
    use sha2::{Digest, Sha256};
    // This fixture never executes its inventory entry or contacts its artifact URL.
    assert!(
        std::env::var_os("SUPERCMD_EXTENSION_PATHS").is_none(),
        "Fixture requires isolated extension discovery"
    );
    let root = tempfile::tempdir().unwrap();
    // Unix legacy grants require an already-canonical registry root. Windows
    // deliberately keeps an ordinary drive spelling to exercise the regression.
    #[cfg(windows)]
    let fixture_root = root.path().to_path_buf();
    #[cfg(not(windows))]
    let fixture_root = root.path().canonicalize().unwrap();
    let client = core(&fixture_root);
    let id = "com.tezbar.native.scope-fixture";
    let package = fixture_root.join("data/extensions/packages").join(id);
    let version = package.join("versions/fixture");
    std::fs::create_dir_all(version.join("bin")).unwrap();
    let executable = b"inventory fixture; never executed";
    std::fs::write(version.join("bin/fixture"), executable).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            version.join("bin/fixture"),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    let hash = format!("{:x}", Sha256::digest(executable));
    let mut manifest = serde_json::json!({"schemaVersion":3,"runtime":"rust","execution":"process","protocolVersion":1,"id":id,"slug":"scope-fixture","name":"Scope fixture","version":"1.0.0","icon":"image.png","capabilities":[],"commands":[{"name":"run"}],"preferences":[],"files":{"bin/fixture":hash},"artifacts":{}});
    for target in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ] {
        manifest["artifacts"][target] = serde_json::json!({"url":"https://example.invalid/never-fetched.zip","sha256":"0".repeat(64),"executable":"bin/fixture","executableSha256":hash});
    }
    let bytes = serde_json::to_vec(&manifest).unwrap();
    std::fs::write(version.join("extension.json"), &bytes).unwrap();
    std::fs::write(package.join("package.json"), &bytes).unwrap();
    std::fs::write(package.join("active.json"),serde_json::to_vec(&serde_json::json!({"directory":"fixture","manifestSha256":format!("{:x}",Sha256::digest(&bytes))})).unwrap()).unwrap();
    let legacy = fixture_root.join("data/extensions/scope-fixture/image.png");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(&legacy, encoded(ImageFormat::Png, 2, 2)).unwrap();
    let scope = client.scope(id.to_owned()).await.unwrap();
    assert!(scope.allowed_legacy.contains(&legacy));
    assert!(
        scope
            .allowed_legacy
            .contains(&legacy.canonicalize().unwrap())
    );
    for path in [legacy.clone(), legacy.canonicalize().unwrap()] {
        let source = tezbar_ui_model::extensions::images::resolve(
            &path.to_string_lossy(),
            &scope.extension_root,
            &scope.allowed_legacy,
        )
        .unwrap();
        let decoded = client
            .load_variant(
                scope.clone(),
                source,
                Effect::ReadFile {
                    token: 1,
                    path,
                    max_bytes: ASSET_BYTES,
                },
                128,
            )
            .await
            .unwrap();
        assert_eq!(decoded.image.size(0).width.0, 2);
    }
    client.0.shutdown().await;
}

#[test]
fn review_n1_geometry_source_and_segment_work_are_bounded() {
    let large = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><path d='M0 0 {}'/></svg>",
        "h1 ".repeat(100_000)
    );
    assert!(svg_preflight(large.as_bytes()).is_err());
    let dense = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><path d='M0 0 {}'/></svg>",
        "h1 ".repeat(16_384)
    );
    assert!(svg_preflight(dense.as_bytes()).is_ok());
    assert!(decode_chat_preview(dense.as_bytes()).is_none());
}

#[test]
fn review_m810_r1_svg_rejects_simultaneous_layers_before_rasterization() {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024">{}<rect x="-8192" y="-8192" width="17408" height="17408"/>{}</svg>"#,
        "<g opacity=\".99\">".repeat(62),
        "</g>".repeat(62)
    );
    assert!(
        svg_tree(svg.as_bytes(), 1024)
            .err()
            .unwrap()
            .contains("layer memory")
    );
    // Siblings release their temporary layer before the next sibling renders.
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024">{}</svg>"#,
        (0..64)
            .map(|i| format!(
                r#"<g opacity=".9"><rect x="{}" y="0" width="8" height="8"/></g>"#,
                i * 12
            ))
            .collect::<String>()
    );
    assert!(decode_svg(svg.as_bytes(), 1024).is_ok());
    let mask=br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><defs><mask id="a"><rect width="32" height="32" fill="white"/></mask></defs><g mask="url(#a)"><rect width="32" height="32" fill="red"/></g></svg>"#;
    assert!(decode_svg(mask, 128).is_ok());
}
#[test]
fn review_m810_r2_stylesheets_are_bounded_before_usvg_css_parser() {
    let css = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><style>{}</style><rect width="32" height="32"/></svg>"#,
        ".a {fill:red}".repeat(60000)
    );
    assert!(
        svg_tree(css.as_bytes(), 1024)
            .err()
            .unwrap()
            .contains("stylesheet")
    );
    let selectors = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><style>{} {{fill:red}}</style></svg>"#,
        (0..2000)
            .map(|i| format!(".c{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert!(
        svg_tree(selectors.as_bytes(), 128)
            .err()
            .unwrap()
            .contains("stylesheet")
    );
    let inline = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg">{}</svg>"#,
        format!(
            r#"<rect width="1" height="1" style="{}fill:red"/>"#,
            " ".repeat(1024)
        )
        .repeat(40)
    );
    assert!(
        svg_tree(inline.as_bytes(), 128)
            .err()
            .unwrap()
            .contains("stylesheet")
    );
    assert!(decode_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><style>.a {fill:red}</style><rect class="a" width="16" height="16"/></svg>"#,128).is_ok());
}
#[test]
fn review_m810_r3_dash_expansion_is_bounded_before_stroking() {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024">{}</svg>"#,
        r#"<path d="M0 0 L1000000 0" stroke="black" stroke-width="1" stroke-dasharray=".1 .1"/>"#
            .repeat(128)
    );
    assert!(
        svg_tree(svg.as_bytes(), 1024)
            .err()
            .unwrap()
            .contains("dash")
    );
    let normal=br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><path d="M0 8 L32 8" stroke="red" stroke-width="2" stroke-dasharray="2 2"/></svg>"#;
    assert!(decode_svg(normal, 128).is_ok());
}
