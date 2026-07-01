// providers.rs — Contrato común de las FUENTES DE CONTEXTO.
//
// El uso de CONTEXTO (ventana de tokens de la sesión activa) se obtiene hoy de
// tres fuentes locales de Claude Code, cada una acoplada a un formato NO oficial:
//   - "hook" (captura del statusLine)   -> claude_code_bridge::parse_capture
//   - "logs" (transcripts .jsonl)       -> claude_log_parser::parse_latest
//   - "sync" (sync.json legado)         -> local_file_watcher::parse_sync
//
// `ContextProvider` define el contrato común (id estable + lectura -> métrica) y
// `all()` es el REGISTRO ordenado por prioridad. El objetivo es desacoplar Quotal
// de los internals de Claude Code: el día que exista una API OFICIAL de uso, será
// SOLO otro provider que implemente este trait y se añada a `all()`, sin tocar el
// resto del pipeline (watcher, estado, frontend).
//
// La prioridad de `all()` refleja la de `SharedState::active()` (hook > logs > sync).

use crate::UsageMetrics;

/// Una fuente de la que Quotal puede leer el uso de contexto de la sesión activa.
pub trait ContextProvider {
    /// Identificador ESTABLE de la fuente. Coincide con `UsageMetrics.source` y con
    /// las clases CSS del frontend: `"hook"` | `"logs"` | `"sync"`.
    fn id(&self) -> &'static str;
    /// Lee la métrica de contexto ACTUAL, o `None` si esta fuente no tiene dato ahora.
    fn read(&self) -> Option<UsageMetrics>;
}

/// Captura oficial del statusLine (lo recomendado cuando el puente está activo).
pub struct HookProvider;
impl ContextProvider for HookProvider {
    fn id(&self) -> &'static str {
        crate::SRC_HOOK
    }
    fn read(&self) -> Option<UsageMetrics> {
        crate::claude_code_bridge::parse_capture()
    }
}

/// Transcripts `.jsonl` de Claude Code (respaldo cuando el puente está apagado).
pub struct LogsProvider;
impl ContextProvider for LogsProvider {
    fn id(&self) -> &'static str {
        crate::SRC_LOGS
    }
    fn read(&self) -> Option<UsageMetrics> {
        crate::claude_log_parser::parse_latest()
    }
}

/// `sync.json` legado (integraciones externas que vuelcan ahí su estado).
pub struct SyncProvider;
impl ContextProvider for SyncProvider {
    fn id(&self) -> &'static str {
        crate::SRC_SYNC
    }
    fn read(&self) -> Option<UsageMetrics> {
        crate::local_file_watcher::parse_sync()
    }
}

/// Registro de proveedores de contexto, EN ORDEN DE PRIORIDAD. Añadir una fuente
/// nueva (p. ej. una futura API oficial) es implementar `ContextProvider` y
/// añadirla aquí; el watcher y el estado ya iteran sobre este registro.
pub fn all() -> [&'static dyn ContextProvider; 3] {
    [&HookProvider, &LogsProvider, &SyncProvider]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_ids_de_los_proveedores_son_estables() {
        // El contrato: cada id coincide con la constante `SRC_*` (y con las clases
        // CSS del frontend). Un cambio aquí rompería la selección de fuente activa.
        assert_eq!(HookProvider.id(), crate::SRC_HOOK);
        assert_eq!(LogsProvider.id(), crate::SRC_LOGS);
        assert_eq!(SyncProvider.id(), crate::SRC_SYNC);
        // `all()` los expone en orden de prioridad (hook > logs > sync).
        let ids: Vec<&str> = all().iter().map(|p| p.id()).collect();
        assert_eq!(ids, vec![crate::SRC_HOOK, crate::SRC_LOGS, crate::SRC_SYNC]);
    }
}
