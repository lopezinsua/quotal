// kimi_bridge.rs
//
// Puente con Kimi Code CLI para el auto-arranque/auto-cierre del widget,
// espejo de `claude_code_bridge.rs` pero sobre `~/.kimi-code/config.toml`.
//
// DIFERENCIAS CLAVE respecto al puente de Claude:
//
//   1. El config de Kimi es TOML, no JSON, y es un fichero DEL USUARIO con su
//      `api_key`, sus `[providers...]`, `[models...]` y sus comentarios. Por eso
//      se edita con `toml_edit` (preserva comentarios y formato originales) y
//      NUNCA se imprime en logs ni tests: solo se toca la tabla `[[hooks]]`.
//   2. Los hooks de Kimi son un array TOML plano: entradas `[[hooks]]` con
//      `event`, `command` y opcionales `matcher`/`timeout`. Las nuestras llevan
//      SOLO `event` y `command`.
//   3. El auto-cierre cuenta procesos `kimi.exe` vivos (cada sesión TUI,
//      `kimi -p` o `kimi web` es un `kimi.exe`) y solo cierra el widget cuando
//      no queda NINGUNA (misma técnica de dos fases que el bridge de Claude).
//
// Escritura ATÓMICA (tmp + rename), idempotente, y al desinstalar se quitan
// SOLO las entradas marcadas (si `hooks` queda vacío se elimina la clave).

use crate::paths;
use std::path::{Path, PathBuf};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

/// Marcas únicas que identifican NUESTROS hooks dentro del `command` de cada
/// entrada `[[hooks]]` (también van en el nombre de los scripts), para
//  instalarlos/quitarlos de forma idempotente sin tocar los hooks del usuario.
const AUTOSTART_MARKER: &str = "quotal-kimi-autostart";
const SHUTDOWN_MARKER: &str = "quotal-kimi-shutdown";

// ---------------------------------------------------------------------------
// Lectura/escritura del config.toml (quirúrgica y atómica)
// ---------------------------------------------------------------------------

/// Lee `~/.kimi-code/config.toml` preservando su formato. Si no existe,
/// devuelve un documento vacío (todavía no hay nada que preservar). Si está
/// CORRUPTO devuelve error limpio: jamás reescribimos un fichero que no
//  entendemos (podríamos destruir la config del usuario).
fn read_config() -> Result<DocumentMut, String> {
    let raw = match std::fs::read_to_string(paths::kimi_config_path()) {
        Ok(r) => r,
        Err(_) => return Ok(DocumentMut::new()),
    };
    raw.parse::<DocumentMut>()
        .map_err(|e| format!("El config.toml de Kimi Code está corrupto; no se modifica: {e}"))
}

/// Escribe el documento en `~/.kimi-code/config.toml` de forma atómica:
/// primero a `<ruta>.toml.tmp` y luego `rename` sobre el destino (atómico
/// dentro del mismo volumen: un corte nunca deja el config a medias).
fn write_config(doc: &DocumentMut) -> Result<(), String> {
    write_atomic(&paths::kimi_config_path(), &doc.to_string())
}

/// Escritura atómica de texto plano (tmp + rename), como la `write_atomic`
/// del bridge de Claude pero para contenido ya serializado.
fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp: PathBuf = path.with_extension("toml.tmp");
    std::fs::write(&tmp, content).map_err(|e| format!("No se pudo escribir el tmp: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        // Limpieza best-effort del tmp si el rename falla.
        let _ = std::fs::remove_file(&tmp);
        format!("No se pudo renombrar sobre {}: {e}", path.display())
    })
}

// ---------------------------------------------------------------------------
// Helpers sobre el array `[[hooks]]` (solo tocamos entradas marcadas)
// ---------------------------------------------------------------------------

/// ¿Esta entrada `[[hooks]]` es de `event` y su `command` lleva el marcador?
fn entry_matches(t: &Table, event: &str, marker: &str) -> bool {
    let ev = t.get("event").and_then(|e| e.as_str());
    let cmd = t.get("command").and_then(|c| c.as_str()).unwrap_or("");
    ev == Some(event) && cmd.contains(marker)
}

/// ¿Alguna entrada `[[hooks]]` de `event` contiene nuestro marcador?
fn event_has_marker(doc: &DocumentMut, event: &str, marker: &str) -> bool {
    doc.get("hooks")
        .and_then(|h| h.as_array_of_tables())
        .map(|tables| tables.iter().any(|t| entry_matches(t, event, marker)))
        .unwrap_or(false)
}

/// Añade una entrada `[[hooks]]` con SOLO `event` y `command` (sin
/// `matcher`/`timeout`), creando el array si no existe. La idempotencia la
/// decide el llamador con `event_has_marker` antes de llamar.
fn append_hook(doc: &mut DocumentMut, event: &str, command: &str) {
    let mut t = Table::new();
    t["event"] = toml_edit::value(event);
    t["command"] = toml_edit::value(command);
    let item = doc.entry("hooks").or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    if !item.is_array_of_tables() {
        // `hooks` existe pero no es un array de tablas (config rara): la
        // reemplazamos; preferimos eso a abortar la instalación.
        *item = Item::ArrayOfTables(ArrayOfTables::new());
    }
    item.as_array_of_tables_mut().unwrap().push(t);
}

/// Quita del array `[[hooks]]` SOLO las entradas marcadas de `event` (deja
/// intactos los hooks del usuario). Si el array queda vacío, elimina la clave
/// `hooks` para no dejar basura en el config.
fn remove_marked(doc: &mut DocumentMut, event: &str, marker: &str) {
    let Some(aot) = doc.get_mut("hooks").and_then(|h| h.as_array_of_tables_mut()) else {
        return;
    };
    let idxs: Vec<usize> = aot
        .iter()
        .enumerate()
        .filter_map(|(i, t)| entry_matches(t, event, marker).then_some(i))
        .collect();
    // Borrado de atrás hacia delante para que los índices sigan siendo válidos.
    for i in idxs.into_iter().rev() {
        aot.remove(i);
    }
    if aot.is_empty() {
        doc.as_table_mut().remove("hooks");
    }
}

/// Reemplaza el `command` de las entradas marcadas de `event` por `new_cmd`.
/// Devuelve true si algún comando cambió de verdad (para no reescribir el
/// config cuando no hace falta).
fn replace_marked_command(doc: &mut DocumentMut, event: &str, marker: &str, new_cmd: &str) -> bool {
    let Some(aot) = doc.get_mut("hooks").and_then(|h| h.as_array_of_tables_mut()) else {
        return false;
    };
    let mut changed = false;
    for t in aot.iter_mut() {
        if !entry_matches(t, event, marker) {
            continue;
        }
        if t.get("command").and_then(|c| c.as_str()) != Some(new_cmd) {
            t["command"] = toml_edit::value(new_cmd);
            changed = true;
        }
    }
    changed
}

// ---------------------------------------------------------------------------
// Auto-arranque: hook `SessionStart` que lanza el widget con Kimi Code
// ---------------------------------------------------------------------------

/// Ruta del script lanzador oculto (Windows). Su nombre contiene el marcador,
/// así que el comando del hook que lo invoca queda detectable por subcadena.
fn autostart_script_path() -> PathBuf {
    paths::widget_dir().join("quotal-kimi-autostart.vbs")
}

/// Escribe un VBScript que arranca el widget SIN abrir ninguna consola
/// (lo ejecuta `wscript`, subsistema GUI), y solo si no hay ya una instancia
/// viva (consulta WMI por el nombre del .exe). Idéntico al del bridge de
/// Claude; solo cambia el nombre del fichero (que lleva el marcador de Kimi).
fn write_autostart_script() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy().to_string();
    let exe_name = exe.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    paths::ensure_widget_dir().map_err(|e| e.to_string())?;

    // `sh.Run "...", 0, False` -> ventana oculta del lanzador (el widget muestra
    // su propia ventana). La query WMI evita duplicar si ya está abierto.
    let vbs = format!(
        "Set sh = CreateObject(\"WScript.Shell\")\r\n\
         On Error Resume Next\r\n\
         Set svc = GetObject(\"winmgmts:\\\\.\\root\\cimv2\")\r\n\
         Set procs = svc.ExecQuery(\"SELECT Name FROM Win32_Process WHERE Name='{exe_name}'\")\r\n\
         n = 0\r\n\
         If Err.Number = 0 Then n = procs.Count\r\n\
         On Error GoTo 0\r\n\
         If n = 0 Then sh.Run \"\"\"{exe_str}\"\"\", 0, False\r\n"
    );

    write_script(&autostart_script_path(), &vbs)
}

/// Construye el comando del hook `SessionStart`. En Windows usa `wscript`
/// sobre el VBS oculto (cero parpadeo de terminal); el marcador va en la ruta
/// del script. En Unix es un `sh -c` con el marcador en un comentario.
fn build_autostart_command() -> Result<String, String> {
    if cfg!(windows) {
        let vbs = write_autostart_script()?;
        Ok(format!("wscript //B \"{}\"", vbs.to_string_lossy()))
    } else {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy().to_string();
        Ok(format!(
            "sh -c 'pgrep -f \"{exe_str}\" >/dev/null 2>&1 || (\"{exe_str}\" >/dev/null 2>&1 &)' # {AUTOSTART_MARKER}"
        ))
    }
}

/// ¿Está instalado el auto-arranque con Kimi Code?
pub fn is_kimi_autostart_installed() -> bool {
    read_config().map(|d| event_has_marker(&d, "SessionStart", AUTOSTART_MARKER)).unwrap_or(false)
}

/// Inyecta (idempotente y atómico) un hook `SessionStart` que abre el widget
/// al iniciar Kimi Code. Conserva cualquier otro hook y el resto del config.
pub fn install_kimi_autostart_hook() -> Result<(), String> {
    let mut doc = read_config()?;

    // Genera/actualiza el lanzador y obtiene el comando del hook (esto
    // reescribe el VBS con la ruta actual del exe aunque el hook ya existiera).
    let command = build_autostart_command()?;

    if event_has_marker(&doc, "SessionStart", AUTOSTART_MARKER) {
        return Ok(()); // hook ya presente; script ya (re)generado arriba
    }

    append_hook(&mut doc, "SessionStart", &command);
    write_config(&doc)
}

/// Elimina nuestro hook `SessionStart` (deja intactos los demás) y limpia la
/// clave `hooks` si queda vacía.
pub fn uninstall_kimi_autostart_hook() -> Result<(), String> {
    let mut doc = read_config()?;
    if !event_has_marker(&doc, "SessionStart", AUTOSTART_MARKER) {
        return Ok(()); // nada que quitar
    }

    remove_marked(&mut doc, "SessionStart", AUTOSTART_MARKER);

    // Borra el lanzador VBS (best-effort).
    let _ = std::fs::remove_file(autostart_script_path());

    write_config(&doc)
}

// ---------------------------------------------------------------------------
// Auto-cierre: hook `SessionEnd` que cierra el widget cuando ya NO queda
// NINGUNA sesión de Kimi Code viva (cada sesión es un proceso `kimi.exe`)
// ---------------------------------------------------------------------------

/// Ruta del script que cierra el widget (Windows). Su nombre lleva el marcador.
fn shutdown_script_path() -> PathBuf {
    paths::widget_dir().join("quotal-kimi-shutdown.vbs")
}

/// Escritura atómica (tmp + rename) de un script del widget.
fn write_script(path: &Path, content: &str) -> Result<PathBuf, String> {
    let tmp = path.with_extension("vbs.tmp");
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })?;
    Ok(path.to_path_buf())
}

/// Escribe un VBScript que cierra el widget SIN abrir consola, y SOLO cuando la
/// sesión que termina era la ÚLTIMA de Kimi Code viva. Con varias sesiones a
/// la vez (dos TUI, un `kimi -p` suelto, un `kimi web`…), el `SessionEnd` de
/// una NO debe llevarse el widget mientras queden otras.
///
/// Dos fases, porque Kimi Code espera a que el comando del hook termine y no
/// queremos retrasar su salida: sin argumentos, el script se relanza a sí mismo
/// desacoplado y devuelve el control YA; la copia diferida espera a que la
/// sesión saliente muera del todo, cuenta los `kimi.exe` que quedan vía WMI y
/// solo si no queda ninguno relanza el widget con `--quit` para el cierre
/// LIMPIO vía single-instance (guardando estado). Si WMI fallara, cierra igual.
fn write_shutdown_script() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy().to_string();
    paths::ensure_widget_dir().map_err(|e| e.to_string())?;

    // `sh.Run "...", 0, False` -> ventana oculta (sin parpadeo de consola). El
    // literal VBScript `"""<exe>"" --quit"` evalúa a `"<exe>" --quit` (ruta entre
    // comillas + argumento). Las comillas dobles internas se escriben como `""`.
    let vbs = format!(
        "Set sh = CreateObject(\"WScript.Shell\")\r\n\
         If WScript.Arguments.Count = 0 Then\r\n\
         \x20 ' Fase 1: relanzarse desacoplado y devolver el control a Kimi Code ya.\r\n\
         \x20 sh.Run \"wscript //B \"\"\" & WScript.ScriptFullName & \"\"\" deferred\", 0, False\r\n\
         \x20 WScript.Quit\r\n\
         End If\r\n\
         ' Fase 2 (diferida): deja salir del todo a la sesion que termina.\r\n\
         WScript.Sleep 3000\r\n\
         ' Quedan otras sesiones de Kimi Code vivas? Entonces el widget se queda.\r\n\
         n = 0\r\n\
         On Error Resume Next\r\n\
         Set svc = GetObject(\"winmgmts:\\\\.\\root\\cimv2\")\r\n\
         Set procs = svc.ExecQuery(\"SELECT ProcessId FROM Win32_Process WHERE Name='kimi.exe'\")\r\n\
         If Err.Number = 0 Then n = procs.Count\r\n\
         On Error GoTo 0\r\n\
         If n = 0 Then sh.Run \"\"\"{exe_str}\"\" --quit\", 0, False\r\n"
    );

    write_script(&shutdown_script_path(), &vbs)
}

/// Construye el comando del hook `SessionEnd`. En Windows usa `wscript` sobre
/// el VBS oculto. En Unix: MEJORA respecto al bridge de Claude —la rama Unix
/// de Claude ya contaba sesiones, y aquí la replicamos contando procesos
/// `kimi` con `pgrep -x` (nombre EXACTO; un `pgrep -f kimi` se auto-detectaría
/// a sí mismo porque el propio comando lleva "kimi" en el comentario del
/// marcador)— y solo si no queda ninguno relanza el exe con `--quit`
/// (cierre LIMPIO vía single-instance), desacoplado para no retrasar la salida.
fn build_shutdown_command() -> Result<String, String> {
    if cfg!(windows) {
        let vbs = write_shutdown_script()?;
        Ok(format!("wscript //B \"{}\"", vbs.to_string_lossy()))
    } else {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy().to_string();
        Ok(format!(
            "sh -c '( sleep 3; pgrep -x kimi >/dev/null 2>&1 || \"{exe_str}\" --quit >/dev/null 2>&1 ) >/dev/null 2>&1 &' # {SHUTDOWN_MARKER}"
        ))
    }
}

/// ¿Está instalado el auto-cierre con Kimi Code?
pub fn is_kimi_shutdown_installed() -> bool {
    read_config().map(|d| event_has_marker(&d, "SessionEnd", SHUTDOWN_MARKER)).unwrap_or(false)
}

/// Inyecta (idempotente y atómico) un hook `SessionEnd` que cierra el widget
/// cuando termina la ÚLTIMA sesión de Kimi Code. Conserva los demás hooks.
pub fn install_kimi_shutdown_hook() -> Result<(), String> {
    let mut doc = read_config()?;

    // (Re)genera el script de cierre con la ruta actual del exe.
    let command = build_shutdown_command()?;

    if event_has_marker(&doc, "SessionEnd", SHUTDOWN_MARKER) {
        return Ok(()); // ya presente; script ya (re)generado arriba
    }

    append_hook(&mut doc, "SessionEnd", &command);
    write_config(&doc)
}

/// Elimina nuestro hook `SessionEnd` (deja intactos los demás) y limpia la
/// clave `hooks` si queda vacía.
pub fn uninstall_kimi_shutdown_hook() -> Result<(), String> {
    let mut doc = read_config()?;
    if !event_has_marker(&doc, "SessionEnd", SHUTDOWN_MARKER) {
        return Ok(()); // nada que quitar
    }

    remove_marked(&mut doc, "SessionEnd", SHUTDOWN_MARKER);
    let _ = std::fs::remove_file(shutdown_script_path());

    write_config(&doc)
}

// ---------------------------------------------------------------------------
// Re-sincronización al arranque (mismo patrón que el bridge de Claude)
// ---------------------------------------------------------------------------

/// Re-sincroniza al arranque los scripts/comandos de los hooks de Kimi YA
/// instalados con la ruta ACTUAL del ejecutable (los `.vbs` incrustan
/// `current_exe()` al instalarse; si la app se mueve/actualiza quedan obsoletos).
///
/// - Windows: `build_*_command` REESCRIBE el `.vbs` con el exe actual (la ruta
///   fija en el config no cambia, así que normalmente no tocamos el TOML).
/// - Unix: el exe va en el propio comando; si cambió, se actualiza.
///
/// Igual que `resync_installed_hooks` del bridge de Claude, esto solo REGENERA
/// lo ya instalado: no instala nada nuevo, así que no se le aplica la guarda de
/// solo-lectura (esa vive en los comandos IPC de instalación, como en Claude).
/// Best-effort: cualquier fallo se ignora (no debe impedir el arranque).
pub fn resync_kimi_hooks() {
    let Ok(mut doc) = read_config() else {
        return; // config corrupto: no lo tocamos
    };
    let mut changed = false;

    if event_has_marker(&doc, "SessionStart", AUTOSTART_MARKER) {
        if let Ok(cmd) = build_autostart_command() {
            changed |= replace_marked_command(&mut doc, "SessionStart", AUTOSTART_MARKER, &cmd);
        }
    }
    if event_has_marker(&doc, "SessionEnd", SHUTDOWN_MARKER) {
        if let Ok(cmd) = build_shutdown_command() {
            changed |= replace_marked_command(&mut doc, "SessionEnd", SHUTDOWN_MARKER, &cmd);
        }
    }

    if changed {
        let _ = write_config(&doc);
    }
}

// ---------------------------------------------------------------------------
// Tests de INTEGRACIÓN contra un `config.toml` REAL en disco (en un HOME
// temporal, nunca el `.kimi-code` real). Es lo que toca el fichero del usuario
// —api_key incluida—: un fallo aquí rompe SU config de Kimi Code. El fixture
// lleva un api_key FALSO solo para garantizar que se preserva; NUNCA se
// imprime el contenido del fichero en aserciones ni logs.
//
// Tocan env vars globales (HOME/USERPROFILE vía `paths`), así que van
// `#[serial]`.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn set_home(dir: &Path) {
        std::env::set_var("HOME", dir);
        std::env::set_var("USERPROFILE", dir);
    }

    fn teardown() {
        std::env::remove_var("HOME");
        std::env::remove_var("USERPROFILE");
    }

    /// Config REALISTA del usuario: comentarios, api_key falsa, secciones de
    /// providers/models y un hook AJENO bajo el MISMO evento que el nuestro.
    const FIXTURE: &str = r#"# Configuración de Kimi Code del usuario (comentario a preservar)
api_key = "sk-FALSO-solo-para-tests"

[providers.moonshot]
base_url = "https://api.moonshot.cn"

# Comentario antes de models
[models.default]
model = "kimi-k2"

# Hook ajeno del usuario: no debe tocarse jamás
[[hooks]]
event = "SessionStart"
command = "echo hook-ajeno"
matcher = "*"
"#;

    fn write_config_file(dir: &Path, content: &str) {
        let kimi = dir.join(".kimi-code");
        std::fs::create_dir_all(&kimi).unwrap();
        std::fs::write(kimi.join("config.toml"), content).unwrap();
    }

    fn read_config_raw(dir: &Path) -> String {
        std::fs::read_to_string(dir.join(".kimi-code/config.toml")).unwrap()
    }

    #[test]
    #[serial]
    fn autostart_install_preserva_secciones_comentarios_y_hooks_ajenos() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        write_config_file(tmp.path(), FIXTURE);

        install_kimi_autostart_hook().unwrap();
        assert!(is_kimi_autostart_installed(), "debería quedar instalado");

        let raw = read_config_raw(tmp.path());
        // Todo lo del usuario sigue ahí (sin imprimirlo: solo presencia).
        assert!(raw.contains("api_key"), "se perdió el api_key del usuario");
        assert!(raw.contains("comentario a preservar"), "se perdieron comentarios");
        assert!(raw.contains("[providers.moonshot]"), "se perdió una sección");
        assert!(raw.contains("echo hook-ajeno"), "se tocó el hook ajeno");
        assert!(raw.contains("matcher = \"*\""), "se alteró la entrada ajena");

        // Nuestra entrada lleva SOLO event y command (sin matcher/timeout).
        let doc = read_config().unwrap();
        let aot = doc["hooks"].as_array_of_tables().unwrap();
        let ours: Vec<_> =
            aot.iter().filter(|t| entry_matches(t, "SessionStart", AUTOSTART_MARKER)).collect();
        assert_eq!(ours.len(), 1);
        let keys: Vec<_> = ours[0].iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec!["event", "command"], "la entrada debe ser mínima");

        teardown();
    }

    #[test]
    #[serial]
    fn autostart_install_es_idempotente() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        write_config_file(tmp.path(), FIXTURE);

        install_kimi_autostart_hook().unwrap();
        install_kimi_autostart_hook().unwrap(); // segunda vez: no debe duplicar

        let doc = read_config().unwrap();
        let aot = doc["hooks"].as_array_of_tables().unwrap();
        let ours =
            aot.iter().filter(|t| entry_matches(t, "SessionStart", AUTOSTART_MARKER)).count();
        assert_eq!(ours, 1, "solo debe existir UNA entrada nuestra");
        // Y el hook ajeno sigue: 2 entradas en total.
        assert_eq!(aot.len(), 2);

        teardown();
    }

    #[test]
    #[serial]
    fn autostart_uninstall_quita_solo_lo_nuestro_y_respeta_el_resto() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        write_config_file(tmp.path(), FIXTURE);

        install_kimi_autostart_hook().unwrap();
        uninstall_kimi_autostart_hook().unwrap();
        assert!(!is_kimi_autostart_installed());

        let raw = read_config_raw(tmp.path());
        // El hook ajeno permanece y todo lo demás está intacto.
        assert!(raw.contains("echo hook-ajeno"), "se borró el hook ajeno");
        assert!(raw.contains("api_key"), "se perdió el api_key");
        assert!(raw.contains("comentario a preservar"));
        let doc = read_config().unwrap();
        let aot = doc["hooks"].as_array_of_tables().unwrap();
        assert_eq!(aot.len(), 1, "solo debe quedar el hook ajeno");

        teardown();
    }

    #[test]
    #[serial]
    fn uninstall_elimina_la_clave_hooks_si_queda_vacia() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        // Config SIN hooks del usuario: al instalar y desinstalar los dos
        // nuestros, la clave `hooks` no debe quedar como basura vacía.
        write_config_file(tmp.path(), "# solo un comentario\napi_key = \"sk-FALSO\"\n");

        install_kimi_autostart_hook().unwrap();
        install_kimi_shutdown_hook().unwrap();
        assert!(is_kimi_autostart_installed() && is_kimi_shutdown_installed());

        uninstall_kimi_autostart_hook().unwrap();
        uninstall_kimi_shutdown_hook().unwrap();

        let raw = read_config_raw(tmp.path());
        assert!(!raw.contains("[[hooks]]"), "no debe quedar ningún hook");
        assert!(!raw.contains("hooks"), "la clave `hooks` vacía debió eliminarse");
        assert!(raw.contains("api_key"), "se perdió el api_key");
        assert!(raw.contains("solo un comentario"), "se perdió el comentario");

        teardown();
    }

    #[test]
    #[serial]
    fn shutdown_roundtrip_preserva_ajenos_y_restaura() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        // Hook ajeno bajo SessionEnd (el mismo evento que el nuestro).
        write_config_file(
            tmp.path(),
            "# config\n[[hooks]]\nevent = \"SessionEnd\"\ncommand = \"echo bye-ajeno\"\n",
        );

        install_kimi_shutdown_hook().unwrap();
        assert!(is_kimi_shutdown_installed());

        uninstall_kimi_shutdown_hook().unwrap();
        assert!(!is_kimi_shutdown_installed());

        let raw = read_config_raw(tmp.path());
        assert!(raw.contains("echo bye-ajeno"), "se tocó el hook ajeno");
        let doc = read_config().unwrap();
        let aot = doc["hooks"].as_array_of_tables().unwrap();
        assert_eq!(aot.len(), 1, "solo debe quedar el ajeno");

        teardown();
    }

    #[test]
    #[serial]
    fn status_detecta_correctamente_con_y_sin_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        // Sin config: no instalado.
        assert!(!is_kimi_autostart_installed());
        assert!(!is_kimi_shutdown_installed());
        // Con hook ajeno solamente: sigue sin detectarse como nuestro.
        write_config_file(tmp.path(), FIXTURE);
        assert!(!is_kimi_autostart_installed());
        assert!(!is_kimi_shutdown_installed());

        teardown();
    }

    #[test]
    #[serial]
    fn toml_corrupto_da_error_limpio_sin_panic() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        write_config_file(tmp.path(), "esto = [no es toml valido\napi_key = \"x\"\n");

        // Instalar: error limpio (NO panic) y el fichero queda intacto.
        assert!(install_kimi_autostart_hook().is_err());
        assert!(install_kimi_shutdown_hook().is_err());
        // Status: no instalado (no explota).
        assert!(!is_kimi_autostart_installed());
        assert!(!is_kimi_shutdown_installed());
        // Desinstalar con config corrupto: error limpio también (jamás
        // reescribimos un fichero que no entendemos) y NO se toca el archivo.
        let antes = read_config_raw(tmp.path());
        assert!(uninstall_kimi_autostart_hook().is_err());
        assert!(uninstall_kimi_shutdown_hook().is_err());
        assert_eq!(read_config_raw(tmp.path()), antes, "no debió tocarse el fichero corrupto");

        teardown();
    }

    #[test]
    #[serial]
    fn uninstall_sin_config_no_falla() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        // No hay config.toml: desinstalar no debe fallar (nada que quitar).
        uninstall_kimi_autostart_hook().unwrap();
        uninstall_kimi_shutdown_hook().unwrap();
        assert!(!is_kimi_autostart_installed());

        teardown();
    }

    #[test]
    #[serial]
    fn resync_regenera_el_script_sin_tocar_el_toml_en_windows() {
        let tmp = tempfile::tempdir().unwrap();
        set_home(tmp.path());
        write_config_file(tmp.path(), "# comentario\n");
        install_kimi_autostart_hook().unwrap();

        let script = autostart_script_path();
        assert!(script.exists(), "el .vbs debe existir tras instalar");
        // Simula una instalación antigua cuyo VBS se perdió.
        std::fs::remove_file(&script).unwrap();

        resync_kimi_hooks();
        assert!(script.exists(), "el resync debe regenerar el .vbs");

        teardown();
    }
}
