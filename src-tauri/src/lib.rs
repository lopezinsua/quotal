// lib.rs — Orquestador: declara los módulos del backend y arranca la app Tauri.
//
// ARQUITECTURA HÍBRIDA (cada pieza en su módulo):
//
//   -state    → tipos del estado compartido y el payload al frontend.
//   -paths    → rutas de archivos (Claude Code + widget).
//   -poller   → las dos tuberías de datos:
//        1. CONTEXTO (offline, dirigido por eventos `notify`).
//        2. LÍMITES DEL PLAN (online, sondeo a `/api/oauth/usage`) con respaldo
//           offline del statusLine y conservación del último dato bueno.
//   -commands → comandos IPC expuestos al frontend.
//   -usage_api / claude_code_bridge / claude_log_parser / local_file_watcher /
//     tray -> dominios concretos (red, puentes, parseo, bandeja).
//
// Ningún watcher ni sondeo bloquea el hilo de la UI: todo vive en tareas de
// `tauri::async_runtime` (tokio).

mod claude_code_bridge;
mod claude_log_parser;
mod commands;
mod local_file_watcher;
pub mod paths;
mod poller;
mod schema_watch;
mod state;
mod tray;
mod usage_api;

// Reexporta los tipos/constantes del estado al raíz del crate, para que el resto
// de módulos los use como `crate::UsageMetrics`, `crate::SRC_HOOK`, etc.
pub use state::*;

use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Punto de entrada de la app
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shared: SharedHandle = Arc::new(Mutex::new(SharedState::default()));

    tauri::Builder::default()
        // Instancia única: si se lanza un segundo proceso (doble clic, autostart
        // simultáneo…), no abre otra ventana ni otro poller; solo trae al frente
        // la existente. Debe registrarse ANTES que el resto de plugins.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            use tauri::Manager;
            // El hook `SessionEnd` relanza el exe con `--quit`; al haber ya una
            // instancia viva, ese argumento llega aquí. Cerramos de forma LIMPIA
            // (guardando el store), no a base de `taskkill /F`.
            if args.iter().any(|a| a == "--quit") {
                crate::tray::quit_clean(app);
                return;
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        // Logging PERSISTENTE: sin esto, en release (subsistema "windows", sin
        // consola) cualquier fallo en casa del usuario sería indiagnosticable.
        // Escribe a `app.log` en el dir de logs del SO (rotado al superar el tope)
        // y, en dev, también a stdout.
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("app".into()),
                    }),
                ])
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000) // ~2 MB por archivo antes de rotar
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(shared.clone())
        .setup(move |app| {
            // Si nos lanzaron solo para cerrar (`--quit`) y resultamos ser la
            // instancia PRIMARIA, no hay ningún widget vivo que cerrar: salimos sin
            // abrir ventana ni montar bandeja/pollers. (Cuando SÍ hay instancia
            // viva, este proceso es secundario y el cierre lo hace el callback de
            // single-instance de arriba.)
            if std::env::args().any(|a| a == "--quit") {
                app.handle().exit(0);
                return Ok(());
            }
            log::info!(
                "Quotal v{} arrancando (logs en el dir de logs del SO)",
                env!("CARGO_PKG_VERSION")
            );
            tray::create_tray(app)?;
            poller::spawn_watchers(app.handle().clone(), shared.clone());
            poller::spawn_plan_poller(app.handle().clone(), shared.clone());
            // Auto-actualización: comprobación silenciosa en segundo plano. Si hay
            // una versión nueva NO la instala; emite `update://available` y es la
            // UI quien decide mostrar el aviso (con botón de instalar). Los errores
            // (sin red, sin update, en `tauri dev`…) solo se registran.
            tauri::async_runtime::spawn(announce_update(app.handle().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_active_mode,
            commands::get_metrics,
            commands::refresh_plan,
            commands::detect_local_sources,
            commands::install_autostart,
            commands::uninstall_autostart,
            commands::autostart_status,
            commands::install_shutdown,
            commands::uninstall_shutdown,
            commands::shutdown_status,
            commands::statusline_status,
            commands::install_statusline_bridge,
            commands::uninstall_statusline_bridge,
            commands::set_tray_static,
            commands::set_bounds,
            commands::animate_bounds,
            commands::get_config,
            commands::update_check,
            commands::update_install,
            commands::check_system_deps,
        ])
        .run(tauri::generate_context!())
        .expect("error al arrancar la aplicación Tauri");
}

/// Comprueba al arranque si hay una versión más reciente y, si la hay, EMITE
/// `update://available` con el estado para que la UI muestre el aviso (con botón
/// de instalar). No instala nada por su cuenta: lo decide el usuario. Cualquier
/// fallo (sin red, endpoint inaccesible, ejecución sin empaquetar en `tauri dev`…)
/// solo se registra, sin afectar al funcionamiento normal.
async fn announce_update(app: tauri::AppHandle) {
    use tauri::Emitter;
    let status = commands::fetch_update_status(&app).await;
    if status.available {
        log::info!("Actualización disponible: v{}", status.version.as_deref().unwrap_or("?"));
        let _ = app.emit("update://available", status);
    } else if let Some(e) = &status.error {
        log::info!("Comprobación de actualización fallida (se ignora): {e}");
    } else {
        log::info!("La app está al día (sin actualizaciones).");
    }
}
