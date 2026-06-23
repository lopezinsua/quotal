// schema_watch.rs — Telemetría LOCAL de "deriva de esquema".
//
// El principal riesgo del proyecto es que Anthropic cambie alguno de los formatos
// NO OFICIALES en los que nos apoyamos (`.credentials.json`, la captura del
// statusLine, la respuesta de `/usage`). Si eso pasa, el widget dejaría de
// funcionar SILENCIOSAMENTE. Este módulo es la red de seguridad: cuando un
// contenedor conocido aparece pero le faltan los campos que esperamos, lo registra
// en `schema_error.log` (junto a la versión) y lo expone a la UI para avisar.
//
// CLAVE — evitar falsos positivos: NO reportamos "el dato no está" (sin login, sin
// suscripción, sesión recién abierta), solo "el contenedor está pero sus campos
// conocidos han desaparecido". Los detectores concretos viven en cada módulo (son
// funciones puras y testeables); aquí solo está el registro + deduplicado + aviso.

use crate::paths;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Derivas ACTIVAS: fuente -> detalle. Una fuente sale del mapa cuando vuelve a
/// parsear bien. Sirve también para deduplicar (solo se escribe al log y se avisa
/// cuando hay un cambio, no en cada sondeo).
fn state() -> &'static Mutex<HashMap<&'static str, String>> {
    static S: OnceLock<Mutex<HashMap<&'static str, String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn log_path() -> std::path::PathBuf {
    paths::widget_dir().join("schema_error.log")
}

/// Reporta el estado de parseo de una fuente:
///   - `Some(detail)` -> DERIVA (contenedor presente, campos esperados ausentes).
///   - `None`         -> OK (parseó bien; limpia cualquier deriva previa).
///
/// Solo escribe al log / avisa cuando hay un CAMBIO real (deriva nueva o con
/// detalle distinto), nunca en cada sondeo, para no spamear el log.
pub fn report(source: &'static str, detail: Option<String>) {
    let Ok(mut map) = state().lock() else { return };
    match detail {
        Some(d) => {
            let changed = map.get(source).map(|prev| prev != &d).unwrap_or(true);
            if changed {
                map.insert(source, d.clone());
                append_log(source, &d);
                log::warn!("[schema] posible cambio de formato en {source}: {d}");
            }
        }
        None => {
            if map.remove(source).is_some() {
                log::info!("[schema] {source} volvió a parsear correctamente");
            }
        }
    }
}

/// Añade una línea al `schema_error.log` (best-effort, append). Incluye marca de
/// tiempo y versión para correlacionar con qué versión de Claude Code rompió.
fn append_log(source: &str, detail: &str) {
    if paths::ensure_widget_dir().is_err() {
        return;
    }
    use std::io::Write;
    let line = format!(
        "{} [v{}] {source}: {detail}\n",
        chrono::Local::now().to_rfc3339(),
        env!("CARGO_PKG_VERSION"),
    );
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Lista de fuentes con deriva activa (ordenada), o `None` si todo va bien. La UI
/// la usa para mostrar un aviso discreto de "quizá necesitas actualizar Quotal".
pub fn warning() -> Option<Vec<String>> {
    let map = state().lock().ok()?;
    if map.is_empty() {
        return None;
    }
    let mut sources: Vec<String> = map.keys().map(|s| s.to_string()).collect();
    sources.sort_unstable();
    Some(sources)
}
