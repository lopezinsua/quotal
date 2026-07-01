// app_config.rs — Configuración del BACKEND que afecta a la SEGURIDAD.
//
// Por ahora solo el modo SOLO-LECTURA (observador). Cuando está activo, Quotal no
// REESCRIBE el token OAuth refrescado en `.credentials.json` ni INSTALA hooks nuevos
// en `settings.json`. El widget sigue leyendo el token y consultando `/usage` con
// normalidad —el refresco vive solo en memoria—, así que funciona igual pero sin
// efectos secundarios sobre los ficheros de Claude Code. Es la garantía de confianza
// más fuerte para quien no quiera que la app toque su credencial.
//
// La fuente de verdad vive en el BACKEND (la capa que hace cumplir la garantía) y se
// carga al ARRANCAR, antes de lanzar el poller, para que ni un solo refresco pueda
// escribir el token antes de conocer la preferencia. El frontend solo la refleja
// (`get_config`) y la cambia (`set_read_only`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static READ_ONLY: AtomicBool = AtomicBool::new(false);

/// ¿Está activo el modo solo-lectura? Lo consultan `usage_api` (write-back del
/// token) y los comandos de instalación de hooks antes de escribir en disco.
pub fn is_read_only() -> bool {
    READ_ONLY.load(Ordering::Relaxed)
}

fn config_path() -> PathBuf {
    crate::paths::widget_dir().join("quotal-config.json")
}

/// Carga la preferencia persistida (best-effort). Llamar UNA vez al arrancar,
/// ANTES de spawnear el poller del plan.
pub fn load() {
    let ro = std::fs::read_to_string(config_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("read_only").and_then(|b| b.as_bool()))
        .unwrap_or(false);
    READ_ONLY.store(ro, Ordering::Relaxed);
}

/// Activa/desactiva el modo solo-lectura y lo persiste de forma atómica.
pub fn set_read_only(enabled: bool) {
    READ_ONLY.store(enabled, Ordering::Relaxed);
    persist(enabled);
}

/// Persiste la preferencia (tmp + rename atómico, best-effort).
fn persist(enabled: bool) {
    if crate::paths::ensure_widget_dir().is_err() {
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(&serde_json::json!({ "read_only": enabled }))
    else {
        return;
    };
    let path = config_path();
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn set_home(dir: &std::path::Path) {
        std::env::set_var("HOME", dir);
        std::env::set_var("USERPROFILE", dir);
    }
    fn teardown() {
        READ_ONLY.store(false, Ordering::Relaxed); // deja el global limpio para otros tests
        std::env::remove_var("HOME");
        std::env::remove_var("USERPROFILE");
    }

    #[test]
    #[serial]
    fn por_defecto_es_false_sin_fichero() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        READ_ONLY.store(true, Ordering::Relaxed); // ensuciamos aposta
        load(); // sin fichero → debe volver a false
        assert!(!is_read_only());
        teardown();
    }

    #[test]
    #[serial]
    fn set_persiste_y_load_lo_recupera() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());

        set_read_only(true);
        assert!(is_read_only());
        // Simula un reinicio: reseteamos el global y recargamos del disco.
        READ_ONLY.store(false, Ordering::Relaxed);
        load();
        assert!(is_read_only(), "la preferencia debe sobrevivir al reinicio");

        set_read_only(false);
        READ_ONLY.store(true, Ordering::Relaxed);
        load();
        assert!(!is_read_only());

        teardown();
    }
}
