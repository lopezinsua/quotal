fn main() {
    // OJO¡ITO: si cambias `icons/icon.ico`, `tauri-build` NO siempre re incrusta el
    // icono del .exe (no rastrea ese archivo como dependencia del build). Para
    // forzar el re-incrustado, toca este `build.rs` (cambia su mtime) o haz
    // `cargo clean`. El icono se genera con `scripts/gen-icon.ps1`.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build()
}
