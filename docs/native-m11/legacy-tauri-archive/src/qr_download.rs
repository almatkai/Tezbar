use base64::{engine::general_purpose::STANDARD, Engine as _};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;

const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn decode_png_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let encoded = data_url
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| "Invalid PNG data URL".to_string())?;
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|_| "Invalid PNG data URL".to_string())?;
    if !bytes.starts_with(PNG_MAGIC) {
        return Err("Invalid PNG data URL".to_string());
    }
    Ok(bytes)
}

/// Save a generated QR bitmap without relying on webview download support.
/// Returns false when the user cancels the native save dialog.
#[tauri::command]
pub async fn save_qr_png_data_url(app: AppHandle, data_url: String) -> Result<bool, String> {
    let bytes = decode_png_data_url(&data_url)?;
    let (sender, receiver) = oneshot::channel();
    app.dialog()
        .file()
        .set_file_name("tezbar-qr.png")
        .add_filter("PNG image", &["png"])
        .save_file(move |path| {
            let _ = sender.send(path);
        });

    let Some(path) = receiver
        .await
        .map_err(|_| "Could not open save dialog".to_string())?
    else {
        return Ok(false);
    };
    let path = path
        .into_path()
        .map_err(|_| "Could not resolve save location".to_string())?;
    std::fs::write(path, bytes).map_err(|error| format!("Could not save QR image: {error}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_png_data_url() {
        let bytes = decode_png_data_url("data:image/png;base64,iVBORw0KGgo=").unwrap();
        assert!(bytes.starts_with(PNG_MAGIC));
    }

    #[test]
    fn rejects_non_png_data_url() {
        assert!(decode_png_data_url("data:image/svg+xml;base64,PHN2Zy8+").is_err());
    }
}
