// updates.rs — Actualizaciones de la app.
//
//   - Comprobación en SEGUNDO PLANO: poco después de arrancar y luego cada pocas
//     horas (un widget puede quedarse abierto días). Se puede apagar desde Ajustes
//     (`app_config::auto_update_check`); la comprobación manual siempre funciona.
//   - Nunca instala por su cuenta: emite `update://available` y la UI ofrece el
//     botón. La actualización encontrada se GUARDA para instalar exactamente esa
//     (sin una segunda consulta que pueda fallar entre medias).
//   - La descarga emite `update://progress` para que la UI muestre el avance.
//   - La autenticidad la garantiza el plugin: verifica la firma minisign del
//     artefacto contra la clave pública de `tauri.conf.json` antes de instalar.

use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Primera comprobación en segundo plano: tras un margen para no competir con
/// el arranque (primer sondeo del plan, watchers…).
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
/// Comprobaciones periódicas mientras la app sigue abierta.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
/// Como mucho un evento de progreso cada este tiempo (la descarga llega en
/// trozos pequeños; no hace falta repintar miles de veces).
const PROGRESS_EVERY: Duration = Duration::from_millis(120);

/// Estado de actualización que viaja al frontend (evento `update://available` y
/// respuesta de `update_check`).
#[derive(Serialize, Clone, Debug, Default)]
pub struct UpdateStatus {
    /// Hay una versión más reciente disponible.
    pub available: bool,
    /// Versión disponible (si la hay).
    pub version: Option<String>,
    /// Versión instalada actualmente.
    pub current: String,
    /// Notas de la versión (Markdown del `latest.json`), si las hay.
    pub notes: Option<String>,
    /// Fecha de publicación (RFC 3339), si el manifiesto la trae.
    pub date: Option<String>,
    /// Mensaje de error si la comprobación falló (sin red, en dev, etc.).
    pub error: Option<String>,
}

impl UpdateStatus {
    fn none(error: Option<String>) -> Self {
        UpdateStatus { current: env!("CARGO_PKG_VERSION").to_string(), error, ..Default::default() }
    }
}

/// Progreso de la instalación (`update://progress`).
#[derive(Serialize, Clone, Debug)]
pub struct UpdateProgress {
    /// "download" mientras se descarga; "install" al terminar la descarga.
    pub phase: &'static str,
    pub downloaded: u64,
    /// Tamaño total si el servidor lo anuncia.
    pub total: Option<u64>,
}

/// Última actualización encontrada (la que se instalará).
fn pending() -> &'static Mutex<Option<Update>> {
    static PENDING: OnceLock<Mutex<Option<Update>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

/// Instante (RFC 3339) de la última comprobación que llegó a responder.
fn last_checked() -> &'static Mutex<Option<String>> {
    static LAST: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

/// Consulta el endpoint del updater SIN instalar nada y recuerda lo encontrado.
pub async fn check(app: &AppHandle) -> UpdateStatus {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => return UpdateStatus::none(Some(e.to_string())),
    };
    let result = updater.check().await;
    if result.is_ok() {
        if let Ok(mut l) = last_checked().lock() {
            *l = Some(chrono::Utc::now().to_rfc3339());
        }
    }
    match result {
        Ok(Some(update)) => {
            let status = UpdateStatus {
                available: true,
                version: Some(update.version.clone()),
                current: update.current_version.clone(),
                notes: update.body.clone().filter(|b| !b.trim().is_empty()),
                date: update.date.map(|d| d.to_string()),
                error: None,
            };
            if let Ok(mut p) = pending().lock() {
                *p = Some(update);
            }
            status
        }
        Ok(None) => {
            if let Ok(mut p) = pending().lock() {
                *p = None;
            }
            UpdateStatus::none(None)
        }
        Err(e) => UpdateStatus::none(Some(e.to_string())),
    }
}

/// Bucle de comprobación en segundo plano. Respeta la preferencia en CADA vuelta,
/// así que apagarla desde Ajustes surte efecto sin reiniciar. Los fallos (sin
/// red, ejecución sin empaquetar en `tauri dev`…) solo se registran.
pub fn spawn_background_checks(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            if crate::app_config::auto_update_check() {
                let status = check(&app).await;
                if status.available {
                    log::info!(
                        "Actualización disponible: v{}",
                        status.version.as_deref().unwrap_or("?")
                    );
                    let _ = app.emit("update://available", status);
                } else if let Some(e) = &status.error {
                    log::info!("Comprobación de actualización fallida (se ignora): {e}");
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

/// Comprobación manual (botón de Ajustes). Funciona aunque la automática esté
/// apagada: es una acción explícita del usuario.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateStatus {
    check(&app).await
}

/// Preferencias de actualización para Ajustes.
#[tauri::command]
pub fn update_prefs() -> serde_json::Value {
    serde_json::json!({
        "auto_check": crate::app_config::auto_update_check(),
        "last_checked": last_checked().lock().ok().and_then(|l| l.clone()),
    })
}

#[tauri::command]
pub fn set_auto_update_check(enabled: bool) {
    crate::app_config::set_auto_update_check(enabled);
}

/// Descarga la actualización encontrada, VERIFICA su firma minisign, la instala y
/// reinicia la app. Emite `update://progress` durante la descarga. Si no hay una
/// guardada (p. ej. la app se abrió hace poco), comprueba antes.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let cached = pending().lock().ok().and_then(|p| p.clone());
    let update = match cached {
        Some(u) => u,
        None => app
            .updater()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no hay actualización disponible".to_string())?,
    };

    let emitter = app.clone();
    let mut downloaded: u64 = 0;
    let mut last_emit: Option<Instant> = None;
    let on_chunk = move |chunk: usize, total: Option<u64>| {
        downloaded += chunk as u64;
        let due = last_emit.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY);
        if due || total == Some(downloaded) {
            last_emit = Some(Instant::now());
            let _ = emitter
                .emit("update://progress", UpdateProgress { phase: "download", downloaded, total });
        }
    };
    let finisher = app.clone();
    let on_finish = move || {
        let _ = finisher.emit(
            "update://progress",
            UpdateProgress { phase: "install", downloaded: 0, total: None },
        );
    };
    update.download_and_install(on_chunk, on_finish).await.map_err(|e| e.to_string())?;
    app.restart()
}
