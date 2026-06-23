// paths.rs — Rutas de archivos del widget y de Claude Code. Sin dependencias
// externas: resuelve el home con variables de entorno (multiplataforma).

use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("HOME") {
        return PathBuf::from(h);
    }
    if let Some(h) = std::env::var_os("USERPROFILE") {
        return PathBuf::from(h);
    }
    PathBuf::from(".")
}

pub fn claude_dir() -> PathBuf {
    home().join(".claude")
}
pub fn settings_path() -> PathBuf {
    claude_dir().join("settings.json")
}
/// Transcripts reales de Claude Code (uso de tokens por turno).
pub fn projects_dir() -> PathBuf {
    claude_dir().join("projects")
}
pub fn widget_dir() -> PathBuf {
    // Directorio de datos LEGADO: se conserva `.claude-usage-widget` (el producto
    // ahora se llama Quotal) para no huérfanar instalaciones previas  los hooks
    // y scripts ya instalados apuntan aquí. Es interno e invisible al usuario.
    home().join(".claude-usage-widget")
}
pub fn sync_path() -> PathBuf {
    widget_dir().join("sync.json")
}
pub fn capture_path() -> PathBuf {
    widget_dir().join("statusline-capture.json")
}
pub fn bridge_backup_path() -> PathBuf {
    widget_dir().join("bridge-backup.json")
}

pub fn ensure_widget_dir() -> std::io::Result<()> {
    std::fs::create_dir_all(widget_dir())
}

/// Segundos transcurridos desde la última modificación del archivo.
pub fn file_age_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX)
}
