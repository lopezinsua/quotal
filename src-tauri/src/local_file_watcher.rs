// local_file_watcher.rs
//
// Fallback automático: vigila de forma pasiva `~/.claude-usage-widget/sync.json`.
// Este archivo es nuestro propio formato (no de Claude Code), pensado para que
// cualquier script externo del usuario pueda "empujar" métricas al widget
// simplemente escribiéndolo. Es la fuente de menor prioridad: solo se usa si
// no hay hook en vivo ni historial reciente.
//
// El acto de vigilar lo orquesta `lib.rs` (que monta el watcher de `notify`
// sobre el directorio del widget); aquí solo vive el contrato de parseo.

use crate::paths;
use crate::{UsageMetrics, SRC_SYNC};
use serde::Deserialize;

/// Esquema esperado de `sync.json`. Todos los campos son opcionales para no
/// romper ante archivos parciales escritos por terceros.
#[derive(Debug, Deserialize)]
struct SyncFile {
    tokens_used: Option<u64>,
    tokens_limit: Option<u64>,
    #[serde(default)]
    label: Option<String>,
}

/// Lee y parsea `sync.json`. Devuelve `None` si no existe o es inválido.
pub fn parse_sync() -> Option<UsageMetrics> {
    let path = paths::sync_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    let parsed: SyncFile = serde_json::from_str(&raw).ok()?;

    let age = paths::file_age_secs(&path);
    let label = parsed.label.unwrap_or_else(|| "Sincronizado (sync.json)".to_string());

    Some(UsageMetrics::from_tokens(SRC_SYNC, &label, parsed.tokens_used, parsed.tokens_limit, age))
}
