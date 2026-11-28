//! Offline QR, EC M, versions 1–10; no remote service or filesystem write.
use base64::{engine::general_purpose::STANDARD, Engine};
use qrcode::{types::Color, EcLevel, QrCode, Version};
fn code(s: &str) -> Result<QrCode, String> {
    if s.is_empty() {
        return Err("Nothing to encode — input is empty.".into());
    }
    if s.len() > 213 {
        return Err("Input is too long for a version 10 QR code.".into());
    }
    for v in 1..=10 {
        let mut bits = qrcode::bits::Bits::new(Version::Normal(v));
        if bits.push_byte_data(s.as_bytes()).is_ok() && bits.push_terminator(EcLevel::M).is_ok() {
            if let Ok(c) = QrCode::with_bits(bits, EcLevel::M) {
                return Ok(c);
            }
        }
    }
    Err("Input is too long for a version 10 QR code.".into())
}
pub fn svg_data_url(s: &str) -> Result<String, String> {
    let c = code(s)?;
    let n = c.width();
    let mut path = String::new();
    for y in 0..n {
        for x in 0..n {
            if c[(x, y)] == Color::Dark {
                path += &format!("M{},{}h1v1h-1z", x + 4, y + 4);
            }
        }
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {0} {0}" shape-rendering="crispEdges"><rect width="100%" height="100%" fill="#fff"/><path d="{path}" fill="#000"/></svg>"##,
        n + 8
    );
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        STANDARD.encode(svg)
    ))
}
pub fn png(s: &str) -> Result<Vec<u8>, String> {
    let c = code(s)?;
    let n = c.width() + 8;
    let img = image::GrayImage::from_fn(512, 512, |x, y| {
        let x = x as usize * n / 512;
        let y = y as usize * n / 512;
        let dark = x >= 4 && y >= 4 && x < n - 4 && y < n - 4 && c[(x - 4, y - 4)] == Color::Dark;
        image::Luma([if dark { 0 } else { 255 }])
    });
    let mut out = std::io::Cursor::new(vec![]);
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}
pub fn png_data_url(s: &str) -> Result<String, String> {
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png(s)?)
    ))
}
