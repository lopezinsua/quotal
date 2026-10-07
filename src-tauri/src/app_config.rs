// app_config.rs — Configuración del BACKEND (la que hace cumplir el propio
// backend, no la UI): el modo solo-lectura y la comprobación automática de
// actualizaciones.
//
// Modo SOLO-LECTURA (observador). Cuando está activo, Quotal no
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

/// ¿Comprobar actualizaciones en segundo plano (al arrancar y cada pocas horas)?
/// Activo por defecto; quien no quiera ninguna conexión a GitHub puede apagarlo
/// y comprobar a mano desde Ajustes.
static AUTO_UPDATE_CHECK: AtomicBool = AtomicBool::new(true);

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
    let v = read_file();
    let flag = |k: &str, default: bool| v.get(k).and_then(|b| b.as_bool()).unwrap_or(default);
    READ_ONLY.store(flag("read_only", false), Ordering::Relaxed);
    AUTO_UPDATE_CHECK.store(flag("auto_update_check", true), Ordering::Relaxed);
}

/// ¿Está activa la comprobación automática de actualizaciones?
pub fn auto_update_check() -> bool {
    AUTO_UPDATE_CHECK.load(Ordering::Relaxed)
}

/// Activa/desactiva la comprobación automática y lo persiste.
pub fn set_auto_update_check(enabled: bool) {
    AUTO_UPDATE_CHECK.store(enabled, Ordering::Relaxed);
    persist("auto_update_check", enabled);
}

/// Contenido actual del fichero de config (objeto vacío si no hay o es ilegible).
fn read_file() -> serde_json::Value {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Activa/desactiva el modo solo-lectura y lo persiste de forma atómica.
pub fn set_read_only(enabled: bool) {
    READ_ONLY.store(enabled, Ordering::Relaxed);
    persist("read_only", enabled);
}

/// Persiste UNA preferencia conservando las demás (tmp + rename atómico,
/// best-effort). Antes se reescribía el fichero con una sola clave.
fn persist(key: &str, enabled: bool) {
    if crate::paths::ensure_widget_dir().is_err() {
        return;
    }
    let mut v = read_file();
    v[key] = serde_json::Value::Bool(enabled);
    let Ok(json) = serde_json::to_string_pretty(&v) else {
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
        AUTO_UPDATE_CHECK.store(true, Ordering::Relaxed);
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

    #[test]
    #[serial]
    fn las_preferencias_se_guardan_sin_pisarse() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        load();
        assert!(auto_update_check(), "la comprobación automática viene activada");

        set_read_only(true);
        set_auto_update_check(false);
        READ_ONLY.store(false, Ordering::Relaxed);
        AUTO_UPDATE_CHECK.store(true, Ordering::Relaxed);
        load();
        assert!(is_read_only(), "guardar otra preferencia no debe borrar read_only");
        assert!(!auto_update_check());

        teardown();
    }
}
