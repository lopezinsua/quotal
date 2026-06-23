// En release, compilar como app de subsistema "windows" (GUI): así el .exe NO
// abre ninguna consola al ejecutarse; solo se ve el widget. En debug seguimos
// con consola para ver logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Punto de entrada. La lógica vive en lib.rs para poder reutilizarla
// en tests y mantener main.rs minimo, siguiendo la convención de Tauri 2.
fn main() {
    quotal_lib::run();
}
