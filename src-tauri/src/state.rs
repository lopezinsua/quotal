// state.rs — Tipos del estado compartido y el payload que viaja al frontend.
//
// `SharedState` agrega las tres fuentes de CONTEXTO (hook/logs/sync) más los
// límites REALES del plan. Se reexporta desde `lib.rs` (`pub use state::*`), de
// modo que el resto del crate lo usa como `crate::UsageMetrics`, `crate::SRC_*`, etc.

use crate::usage_api;
use serde::Serialize;
use std::sync::{Arc, Mutex};

// Identificadores de fuente, compartidos con el frontend (clases CSS).
pub const SRC_HOOK: &str = "hook";
pub const SRC_LOGS: &str = "logs";
pub const SRC_SYNC: &str = "sync";
pub const SRC_NONE: &str = "none";

/// A partir de cuántos segundos consideramos un dato "antiguo" (atenuado).
pub const STALE_AFTER_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize)]
pub struct UsageMetrics {
    /// "hook" | "logs" | "sync" | "none" — mapea a clases CSS del frontend.
    pub source: String,
    /// Etiqueta legible de procedencia ("En directo vía Terminal", …).
    pub label: String,
    pub tokens_used: Option<u64>,
    pub tokens_limit: Option<u64>,
    pub percent_used: Option<f32>,
    pub percent_remaining: Option<f32>,
    /// Antigüedad del dato en segundos (frescura).
    pub last_seen_secs: u64,
    /// `true` si el dato es antiguo; el frontend lo atenúa.
    pub stale: bool,
    /// Marca de tiempo local en la que el backend consolidó el dato.
    pub updated_at: String,
}

impl UsageMetrics {
    pub fn from_tokens(
        source: &str,
        label: &str,
        used: Option<u64>,
        limit: Option<u64>,
        last_seen_secs: u64,
    ) -> Self {
        let percent_used = match (used, limit) {
            (Some(u), Some(l)) if l > 0 => Some((u as f32 / l as f32 * 100.0).clamp(0.0, 100.0)),
            _ => None,
        };
        UsageMetrics {
            source: source.to_string(),
            label: label.to_string(),
            tokens_used: used,
            tokens_limit: limit,
            percent_used,
            percent_remaining: percent_used.map(|p| 100.0 - p),
            last_seen_secs,
            stale: last_seen_secs > STALE_AFTER_SECS,
            updated_at: chrono::Local::now().to_rfc3339(),
        }
    }

    pub(crate) fn none() -> Self {
        UsageMetrics {
            source: SRC_NONE.to_string(),
            label: "Sin fuentes locales".to_string(),
            tokens_used: None,
            tokens_limit: None,
            percent_used: None,
            percent_remaining: None,
            last_seen_secs: u64::MAX,
            stale: true,
            updated_at: chrono::Local::now().to_rfc3339(),
        }
    }
}

#[derive(Default)]
pub struct SharedState {
    pub(crate) hook: Option<UsageMetrics>,
    pub(crate) logs: Option<UsageMetrics>,
    pub(crate) sync: Option<UsageMetrics>,
    /// Datos REALES de límites del plan (vía endpoint `/usage`).
    pub(crate) plan: usage_api::PlanInfo,
}

/// Payload unificado emitido al frontend: límites reales del plan + contexto
/// (ventana de tokens, fuente activa).
#[derive(Debug, Clone, Serialize)]
pub struct MetricsPayload {
    pub active: UsageMetrics,
    pub plan: usage_api::PlanInfo,
    /// Fuentes cuyo formato parece haber cambiado (deriva de esquema de Claude
    /// Code). `None` si todo parsea bien. La UI lo usa para avisar al usuario.
    pub schema_warning: Option<Vec<String>>,
}

impl SharedState {
    pub(crate) fn payload(&self) -> MetricsPayload {
        MetricsPayload {
            active: self.active(),
            plan: self.plan.clone(),
            schema_warning: crate::schema_watch::warning(),
        }
    }

    /// Selecciona la fuente activa por prioridad (hook > logs > sync),
    /// prefiriendo siempre datos no-stale; si todos están stale, devuelve el
    /// de mayor prioridad disponible.
    pub(crate) fn active(&self) -> UsageMetrics {
        let ordered = [self.hook.as_ref(), self.logs.as_ref(), self.sync.as_ref()];
        if let Some(m) = ordered.iter().flatten().find(|m| !m.stale) {
            return (*m).clone();
        }
        if let Some(m) = ordered.iter().flatten().next() {
            return (*m).clone();
        }
        UsageMetrics::none()
    }
}

pub type SharedHandle = Arc<Mutex<SharedState>>;
