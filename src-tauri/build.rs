fn main() {
    println!("cargo:rerun-if-changed=../dist-backend/main.js");
    tauri_build::build()
}
