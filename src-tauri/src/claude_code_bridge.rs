// claude_code_bridge.rs
//
// Puente PASIVO con Claude Code. No hace red ni guarda secretos. Sus tres
// responsabilidades:
//
//   1. Detectar el binario `claude` en el PATH (informativo).
//   2. Leer / modificar de forma ATÓMICA `~/.claude/settings.json` para
//      inyectar (o remover) un hook `statusLine` que vuelca el JSON de
//      sesión que Claude Code emite hacia un archivo temporal del widget.
//   3. Interpretar de forma pasiva ese archivo temporal cuando Claude Code
//      lo reescribe, traduciéndolo a `UsageMetrics`.
//
// La escritura de `settings.json` se hace con la técnica tmp + rename para
// no corromper el archivo del usuario ante un cierre inesperado, y siempre
// encadena cualquier `statusLine.command` preexistente.

use crate::paths;
use crate::{UsageMetrics, SRC_HOOK};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Subcadena que delata que el comando de statusLine es nuestro, para
/// detectarlo de forma idempotente. Aparece en la ruta del script lanzador
/// (`statusline-capture.cjs`) y nunca en el statusline normal del usuario.
const BRIDGE_MARKER: &str = "statusline-capture";

/// Ruta del wrapper Node que captura el JSON del statusLine y reenvía al
/// statusline original del usuario.
fn statusline_script_path() -> PathBuf {
    paths::widget_dir().join("statusline-capture.cjs")
}

// ---------------------------------------------------------------------------
// 1. Detección del binario
// ---------------------------------------------------------------------------

/// Busca `claude` en el PATH sin lanzar ningún proceso (cero red, cero shell).
/// Devuelve la primera ruta encontrada o `None`.
pub fn detect_claude_binary() -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    let candidates = if cfg!(windows) {
        vec!["claude.exe", "claude.cmd", "claude.bat", "claude"]
    } else {
        vec!["claude"]
    };
    for dir in std::env::split_paths(&path_var) {
        for name in &candidates {
            let full = dir.join(name);
            if full.is_file() {
                return Some(full.to_string_lossy().to_string());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 2. Escritura atómica de settings.json + hook statusLine
// ---------------------------------------------------------------------------

/// Lee `settings.json` como JSON. Si no existe o está corrupto devuelve un
/// objeto vacío (no es un error: simplemente todavía no hay ajustes).
fn read_settings() -> Value {
    match std::fs::read_to_string(paths::settings_path()) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    }
}

/// Escribe `value` en `path` de forma atómica: primero a `<path>.tmp` y luego
/// `rename` sobre el destino. `rename` es atómico dentro del mismo volumen,
/// así que un corte de luz nunca deja el `settings.json` a medias.
fn write_atomic(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp: PathBuf = path.with_extension("json.tmp");
    let pretty = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, pretty).map_err(|e| format!("No se pudo escribir el tmp: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        // Limpieza best-effort del tmp si el rename falla.
        let _ = std::fs::remove_file(&tmp);
        format!("No se pudo renombrar sobre {}: {e}", path.display())
    })
}

/// Extrae el `statusLine.command` actual, si lo hay y NO es ya nuestro.
fn existing_foreign_command(settings: &Value) -> Option<String> {
    let cmd = settings.get("statusLine")?.get("command")?.as_str()?.trim();
    if cmd.is_empty() || cmd.contains(BRIDGE_MARKER) {
        None
    } else {
        Some(cmd.to_string())
    }
}

/// ¿Está ya instalado nuestro puente?
pub fn is_bridge_installed() -> bool {
    read_settings()
        .get("statusLine")
        .and_then(|s| s.get("command"))
        .and_then(|c| c.as_str())
        .map(|c| c.contains(BRIDGE_MARKER))
        .unwrap_or(false)
}

/// Genera el wrapper Node que: (1) lee el JSON del statusLine de stdin, (2) lo
/// vuelca al archivo de captura (datos OFICIALES de contexto + rate_limits) y
/// (3) reenvía ese mismo JSON al statusline original del usuario propagando su
/// stdout, su stderr Y su código de salida (antes se perdían stderr y el exit).
/// La captura se escribe de forma ASÍNCRONA para no añadir latencia perceptible
/// al prompt de Claude Code. Es seguro en Windows (no usa `tee`) y multiplataforma.
fn write_statusline_script(foreign: Option<&str>) -> Result<PathBuf, String> {
    paths::ensure_widget_dir().map_err(|e| e.to_string())?;
    let cap = paths::capture_path().to_string_lossy().to_string();
    // Literales JS bien escapados (rutas con backslashes, comillas, etc.).
    let cap_lit = serde_json::to_string(&cap).unwrap_or_else(|_| "\"\"".into());
    let orig_lit = match foreign {
        Some(o) => serde_json::to_string(o).unwrap_or_else(|_| "null".into()),
        None => "null".to_string(),
    };

    let script = format!(
        "// Auto-generado por claude-usage-widget. No editar.\n\
const fs = require('fs');\n\
const {{ spawnSync }} = require('child_process');\n\
const CAPTURE = {cap_lit};\n\
const ORIG = {orig_lit};\n\
let data = '';\n\
process.stdin.setEncoding('utf8');\n\
process.stdin.on('data', (c) => (data += c));\n\
process.stdin.on('end', () => {{\n\
  // Captura para el widget: ASÍNCRONA y best-effort, para no retrasar el statusline\n\
  // (Node drena la cola de eventos antes de salir, así que igualmente se escribe).\n\
  fs.writeFile(CAPTURE, data, () => {{}});\n\
  if (!ORIG) return;\n\
  // Reenvía al statusline original propagando stdout, stderr Y código de salida.\n\
  try {{\n\
    const r = spawnSync(ORIG, {{ shell: true, input: data, encoding: 'utf8' }});\n\
    if (r.stdout) process.stdout.write(r.stdout);\n\
    if (r.stderr) process.stderr.write(r.stderr);\n\
    process.exitCode = typeof r.status === 'number' ? r.status : 0;\n\
  }} catch (e) {{\n\
    process.exitCode = 1;\n\
  }}\n\
}});\n"
    );

    let path = statusline_script_path();
    let tmp = path.with_extension("cjs.tmp");
    std::fs::write(&tmp, script).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })?;
    Ok(path)
}

/// Instala el puente statusLine de forma idempotente y atómica. Envuelve el
/// statusline original del usuario (sin romperlo) con un wrapper Node que
/// captura el JSON oficial de Claude Code.
pub fn install_statusline_bridge() -> Result<(), String> {
    paths::ensure_widget_dir().map_err(|e| e.to_string())?;
    let mut settings = read_settings();

    if is_bridge_installed() {
        return Ok(()); // ya está; nada que hacer
    }

    let foreign = existing_foreign_command(&settings);

    // Copia del statusLine original para restaurarlo tal cual al desinstalar.
    let backup = json!({ "previous": settings.get("statusLine").cloned() });
    write_atomic(&paths::bridge_backup_path(), &backup)?;

    let script = write_statusline_script(foreign.as_deref())?;
    let injected = format!("node \"{}\"", script.to_string_lossy());

    if !settings.is_object() {
        settings = json!({});
    }
    settings["statusLine"] = json!({
        "type": "command",
        "command": injected,
    });

    write_atomic(&paths::settings_path(), &settings)
}

/// Remueve el hook, restaurando el `statusLine` previo desde el backup (o
/// eliminándolo si no había). También atómico.
pub fn uninstall_statusline_bridge() -> Result<(), String> {
    let mut settings = read_settings();

    let previous = std::fs::read_to_string(paths::bridge_backup_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|b| b.get("previous").cloned());

    match previous {
        Some(Value::Null) | None => {
            if let Some(obj) = settings.as_object_mut() {
                obj.remove("statusLine");
            }
        }
        Some(prev) => {
            settings["statusLine"] = prev;
        }
    }

    write_atomic(&paths::settings_path(), &settings)?;
    let _ = std::fs::remove_file(paths::bridge_backup_path());
    let _ = std::fs::remove_file(statusline_script_path());
    Ok(())
}

// ---------------------------------------------------------------------------
// 2.b  Auto-arranque: hook `SessionStart` que lanza el widget con Claude Code
// ---------------------------------------------------------------------------

/// Marca única que identifica NUESTRO hook `SessionStart` dentro de
/// `settings.json`, para instalarlo/quitarlo de forma idempotente sin tocar
/// otros hooks del usuario.
const AUTOSTART_MARKER: &str = "claude-usage-widget-autostart";

/// Ruta del script lanzador oculto (Windows). Su nombre contiene el marcador,
/// así que el comando del hook que lo invoca queda detectable por subcadena.
fn autostart_script_path() -> PathBuf {
    paths::widget_dir().join("claude-usage-widget-autostart.vbs")
}

/// Escribe un script VBScript que arranca el widget SIN abrir ninguna consola
/// (lo ejecuta `wscript`, que es subsistema GUI), y solo si no hay ya una
/// instancia viva (consulta WMI por el nombre del .exe). Devuelve su ruta.
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

    let path = autostart_script_path();
    let tmp = path.with_extension("vbs.tmp");
    std::fs::write(&tmp, vbs).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })?;
    Ok(path)
}

/// Construye el comando del hook `SessionStart`. En Windows usa `wscript` sobre
/// el VBS oculto (cero parpadeo de terminal). El comando contiene el marcador
/// vía la ruta del script.
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

// ---- Helpers genéricos de hooks de evento (SessionStart / SessionEnd / …) ----
// Tanto el auto-arranque como el auto-cierre AÑADEN un grupo de hook propio bajo
// un evento y, al desactivar, quitan SOLO el suyo. Así NUNCA se tocan los demás
// hooks del usuario: desactivar deja `settings.json` exactamente como estaba
// antes de activar (la "configuración anterior" que se pide restaurar).

/// ¿Este grupo de hooks contiene un comando con el marcador dado?
fn group_has_marker(group: &Value, marker: &str) -> bool {
    group
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|hs| {
            hs.iter().any(|h| {
                h.get("command")
                    .and_then(|c| c.as_str())
                    .map(|c| c.contains(marker))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// ¿Algún grupo de `hooks.<event>` contiene nuestro marcador?
fn event_has_marker(settings: &Value, event: &str, marker: &str) -> bool {
    settings
        .get("hooks")
        .and_then(|h| h.get(event))
        .and_then(|s| s.as_array())
        .map(|groups| groups.iter().any(|g| group_has_marker(g, marker)))
        .unwrap_or(false)
}

/// Añade un grupo de hook con `command` bajo `hooks.<event>`, creando los
/// contenedores que falten. La idempotencia la decide el llamador con
/// `event_has_marker` antes de llamar.
fn append_hook_group(settings: &mut Value, event: &str, command: String) {
    if !settings.is_object() {
        *settings = json!({});
    }
    let group = json!({ "hooks": [ { "type": "command", "command": command } ] });
    let root = settings.as_object_mut().unwrap();
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let arr = hooks.as_object_mut().unwrap().entry(event).or_insert_with(|| json!([]));
    if !arr.is_array() {
        *arr = json!([]);
    }
    arr.as_array_mut().unwrap().push(group);
}

/// Quita de `hooks.<event>` los grupos marcados (deja intactos los demás) y
/// limpia los contenedores que queden vacíos para no dejar basura.
fn remove_marked_group(settings: &mut Value, event: &str, marker: &str) {
    if let Some(groups) =
        settings.get_mut("hooks").and_then(|h| h.get_mut(event)).and_then(|s| s.as_array_mut())
    {
        groups.retain(|g| !group_has_marker(g, marker));
    }
    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        let empty =
            hooks.get(event).and_then(|s| s.as_array()).map(|a| a.is_empty()).unwrap_or(false);
        if empty {
            hooks.remove(event);
        }
    }
    if settings.get("hooks").and_then(|h| h.as_object()).map(|h| h.is_empty()).unwrap_or(false) {
        if let Some(obj) = settings.as_object_mut() {
            obj.remove("hooks");
        }
    }
}

/// Reemplaza el `command` del hook marcado bajo `hooks.<event>` por `new_cmd`.
/// Devuelve true si algún comando cambió de verdad (para no reescribir
/// `settings.json` cuando no hace falta).
fn replace_marked_command(settings: &mut Value, event: &str, marker: &str, new_cmd: &str) -> bool {
    let Some(groups) =
        settings.get_mut("hooks").and_then(|h| h.get_mut(event)).and_then(|s| s.as_array_mut())
    else {
        return false;
    };
    let mut changed = false;
    for group in groups.iter_mut() {
        let Some(hooks) = group.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
            continue;
        };
        for h in hooks.iter_mut() {
            let is_ours = h
                .get("command")
                .and_then(|c| c.as_str())
                .map(|c| c.contains(marker))
                .unwrap_or(false);
            if !is_ours {
                continue;
            }
            let differs =
                h.get("command").and_then(|c| c.as_str()).map(|c| c != new_cmd).unwrap_or(true);
            if differs {
                if let Some(obj) = h.as_object_mut() {
                    obj.insert("command".into(), Value::String(new_cmd.to_string()));
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Re-sincroniza al arranque los scripts/comandos de los hooks YA instalados con
/// la ruta ACTUAL del ejecutable. Los `.vbs` (Windows) incrustan `current_exe()`
/// en el momento de instalar; si la app se mueve, se renombra su carpeta o se
/// actualiza, esa ruta queda obsoleta y el hook deja de funcionar silenciosamente.
/// Aquí, en cada arranque:
///
/// - Windows: `build_*_command` REESCRIBE el `.vbs` con el exe actual (su ruta
///   fija en `settings.json` no cambia, así que normalmente no tocamos el JSON).
/// - Unix: el exe va en el propio comando de `settings.json`; si cambió, lo
///   actualizamos.
///
/// Esto también propaga a instalaciones antiguas la versión nueva de los scripts
/// (p. ej. el cierre limpio con `--quit` en vez del viejo `taskkill /F`).
/// Best-effort: cualquier fallo se ignora (no debe impedir el arranque).
pub fn resync_installed_hooks() {
    let mut settings = read_settings();
    let mut changed = false;

    if event_has_marker(&settings, "SessionStart", AUTOSTART_MARKER) {
        if let Ok(cmd) = build_autostart_command() {
            changed |=
                replace_marked_command(&mut settings, "SessionStart", AUTOSTART_MARKER, &cmd);
        }
    }
    if event_has_marker(&settings, "SessionEnd", SHUTDOWN_MARKER) {
        if let Ok(cmd) = build_shutdown_command() {
            changed |= replace_marked_command(&mut settings, "SessionEnd", SHUTDOWN_MARKER, &cmd);
        }
    }

    if changed {
        let _ = write_atomic(&paths::settings_path(), &settings);
    }
}

/// ¿Está instalado el auto-arranque con Claude Code?
pub fn is_autostart_installed() -> bool {
    event_has_marker(&read_settings(), "SessionStart", AUTOSTART_MARKER)
}

/// Inyecta (idempotente y atómico) un hook `SessionStart` que abre el widget al
/// iniciar Claude Code. Conserva cualquier otro hook `SessionStart` existente.
pub fn install_autostart_hook() -> Result<(), String> {
    let mut settings = read_settings();

    // Genera/actualiza el lanzador y obtiene el comando del hook (esto reescribe
    // el VBS con la ruta actual del exe aunque el hook ya existiera).
    let command = build_autostart_command()?;

    if event_has_marker(&settings, "SessionStart", AUTOSTART_MARKER) {
        return Ok(()); // hook ya presente; script ya (re)generado arriba
    }

    append_hook_group(&mut settings, "SessionStart", command);
    write_atomic(&paths::settings_path(), &settings)
}

/// Elimina nuestro hook `SessionStart` (deja intactos los demás) y limpia los
/// contenedores que queden vacíos.
pub fn uninstall_autostart_hook() -> Result<(), String> {
    let mut settings = read_settings();
    if !event_has_marker(&settings, "SessionStart", AUTOSTART_MARKER) {
        return Ok(()); // nada que quitar
    }

    remove_marked_group(&mut settings, "SessionStart", AUTOSTART_MARKER);

    // Borra el lanzador VBS (best-effort).
    let _ = std::fs::remove_file(autostart_script_path());

    write_atomic(&paths::settings_path(), &settings)
}

// ---------------------------------------------------------------------------
// 2.c  Auto-cierre: hook `SessionEnd` que cierra el widget al salir de Claude Code
// ---------------------------------------------------------------------------

/// Marca única de NUESTRO hook `SessionEnd` (mismo patrón idempotente que el
/// auto-arranque). Aparece en la ruta del script de cierre.
const SHUTDOWN_MARKER: &str = "claude-usage-widget-shutdown";

/// Ruta del script que cierra el widget (Windows). Su nombre lleva el marcador.
fn shutdown_script_path() -> PathBuf {
    paths::widget_dir().join("claude-usage-widget-shutdown.vbs")
}

/// Escribe un VBScript que cierra el widget SIN abrir consola, relanzándolo con
/// `--quit`: la instancia única viva recibe la señal (vía single-instance) y se
/// cierra de forma LIMPIA guardando su estado, en vez de un `taskkill /F` que la
/// mataba sin guardar. Lanzado oculto por `wscript`. Devuelve su ruta.
fn write_shutdown_script() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy().to_string();
    paths::ensure_widget_dir().map_err(|e| e.to_string())?;

    // `sh.Run "...", 0, False` -> ventana oculta (sin parpadeo de consola). El
    // literal VBScript `"""<exe>"" --quit"` evalúa a `"<exe>" --quit` (ruta entre
    // comillas + argumento). Las comillas dobles internas se escriben como `""`.
    let vbs = format!(
        "Set sh = CreateObject(\"WScript.Shell\")\r\n\
         sh.Run \"\"\"{exe_str}\"\" --quit\", 0, False\r\n"
    );

    let path = shutdown_script_path();
    let tmp = path.with_extension("vbs.tmp");
    std::fs::write(&tmp, vbs).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })?;
    Ok(path)
}

/// Construye el comando del hook `SessionEnd`. En Windows usa `wscript` sobre el
/// VBS oculto; en Unix `pkill -f` sobre la ruta del exe (el marcador va en un
/// comentario para que sea detectable por subcadena).
fn build_shutdown_command() -> Result<String, String> {
    if cfg!(windows) {
        let vbs = write_shutdown_script()?;
        Ok(format!("wscript //B \"{}\"", vbs.to_string_lossy()))
    } else {
        // Relanza el exe con `--quit`: la instancia viva lo recibe vía
        // single-instance y se cierra LIMPIAMENTE (no `pkill`, que la mataría sin
        // guardar y por patrón de ruta podría alcanzar procesos no deseados).
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_str = exe.to_string_lossy().to_string();
        Ok(format!("sh -c '\"{exe_str}\" --quit >/dev/null 2>&1' # {SHUTDOWN_MARKER}"))
    }
}

/// Está instalado el auto-cierre con Claude Code?
pub fn is_shutdown_installed() -> bool {
    event_has_marker(&read_settings(), "SessionEnd", SHUTDOWN_MARKER)
}

/// Inyecta (idempotente y atómico) un hook `SessionEnd` que cierra el widget al
/// terminar la sesión de Claude Code. Conserva los demás hooks `SessionEnd`.
pub fn install_shutdown_hook() -> Result<(), String> {
    let mut settings = read_settings();

    // (Re)genera el script de cierre con la ruta actual del exe.
    let command = build_shutdown_command()?;

    if event_has_marker(&settings, "SessionEnd", SHUTDOWN_MARKER) {
        return Ok(()); // ya presente; script ya (re)generado arriba
    }

    append_hook_group(&mut settings, "SessionEnd", command);
    write_atomic(&paths::settings_path(), &settings)
}

/// Elimina nuestro hook `SessionEnd` (deja intactos los demás) y limpia los
/// contenedores vacíos.
pub fn uninstall_shutdown_hook() -> Result<(), String> {
    let mut settings = read_settings();
    if !event_has_marker(&settings, "SessionEnd", SHUTDOWN_MARKER) {
        return Ok(()); // nada que quitar
    }

    remove_marked_group(&mut settings, "SessionEnd", SHUTDOWN_MARKER);
    let _ = std::fs::remove_file(shutdown_script_path());

    write_atomic(&paths::settings_path(), &settings)
}

// ---------------------------------------------------------------------------
// 3. Interpretación pasiva del archivo de captura
// ---------------------------------------------------------------------------

/// Parsea el JSON de sesión volcado por el hook. Es best-effort: Claude Code
/// puede cambiar el esquema, así que buscamos varios nombres de campo
/// plausibles y degradamos con elegancia si no están.
pub fn parse_capture() -> Option<UsageMetrics> {
    let path = paths::capture_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    let value = last_json_value(&raw)?;
    // Telemetría de deriva: la captura está y es JSON, así que si un contenedor
    // conocido (`context_window`/`rate_limits`) perdió sus campos, avisamos.
    crate::schema_watch::report("statusline", capture_drift_detail(&value));
    let age = paths::file_age_secs(&path);
    metrics_from_capture(&value, age)
}

/// Detector PURO de deriva en la captura del statusLine: solo marca deriva cuando
/// un contenedor que SÍ esperamos (`context_window` o `rate_limits`) está presente
/// pero le faltan sus campos conocidos. Si el contenedor no está (sesión recién
/// abierta, sin primera respuesta) → `None`: no es deriva, es "aún no hay dato".
fn capture_drift_detail(v: &Value) -> Option<String> {
    if let Some(cw) = v.get("context_window") {
        let known = cw.get("context_window_size").is_some()
            || cw.get("total_input_tokens").is_some()
            || cw.get("current_usage").is_some();
        if !known {
            return Some("`context_window` presente pero sin campos de tokens conocidos".into());
        }
    }
    if let Some(rl) = v.get("rate_limits") {
        let known = rl.get("five_hour").is_some() || rl.get("seven_day").is_some();
        if !known {
            return Some("`rate_limits` presente pero sin five_hour/seven_day".into());
        }
    }
    None
}

/// El hook puede escribir varias líneas; nos quedamos con el ÚLTIMO objeto JSON
/// válido (el estado más reciente). Si no hay líneas válidas, intenta el todo.
fn last_json_value(raw: &str) -> Option<Value> {
    raw.lines()
        .rev()
        .find_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .or_else(|| serde_json::from_str(raw).ok())
}

/// Convierte el JSON del statusLine en métricas de contexto. Función PURA (sin
/// tocar disco) para poder testearla. Prefiere el objeto OFICIAL
/// `context_window`; si no está, cae a esquemas antiguos.
fn metrics_from_capture(value: &Value, age: u64) -> Option<UsageMetrics> {
    // FUENTE OFICIAL (Claude Code v2.1.132+): el JSON de statusLine trae un
    // objeto `context_window` con el tamaño REAL de la ventana (200k o 1M según
    // el modelo) y el % ya calculado por Claude. Lo preferimos sobre cualquier
    // estimación: así el medidor coincide exactamente con la terminal.
    if let Some(cw) = value.get("context_window") {
        let limit = cw
            .get("context_window_size")
            .and_then(|v| v.as_u64())
            .filter(|&n| n > 0)
            .unwrap_or(200_000);
        // `total_input_tokens` = input + cache_creation + cache_read: la misma
        // fórmula (solo-entrada) con la que Claude obtiene `used_percentage`.
        // (Los "cache" tokens son partes del prompt servidas desde la caché de
        // prompts de Anthropic; siguen ocupando contexto, por eso se suman.)
        let used = cw.get("total_input_tokens").and_then(|v| v.as_u64()).or_else(|| {
            cw.get("current_usage").map(|u| {
                let f = |k: &str| u.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
                f("input_tokens") + f("cache_creation_input_tokens") + f("cache_read_input_tokens")
            })
        });
        if let Some(used) = used {
            return Some(UsageMetrics::from_tokens(
                SRC_HOOK,
                "Contexto · oficial (Claude Code)",
                Some(used),
                Some(limit),
                age,
            ));
        }
    }

    // Respaldo (esquemas antiguos o señal binaria de 200k).
    let limit: u64 = 200_000;
    let used = extract_tokens(value).or_else(|| {
        if value.get("exceeds_200k_tokens")?.as_bool()? {
            Some(limit)
        } else {
            None
        }
    });
    Some(UsageMetrics::from_tokens(
        SRC_HOOK,
        "En directo vía Terminal",
        used,
        used.map(|_| limit),
        age,
    ))
}

/// Suma cualquier combinación de campos de tokens que aparezca en el JSON.
fn extract_tokens(value: &Value) -> Option<u64> {
    // Posibles ubicaciones: top-level o dentro de `usage`/`cost`.
    let buckets = [Some(value), value.get("usage"), value.get("cost")];
    let keys = [
        "total_tokens",
        "tokens",
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ];
    let mut total: u64 = 0;
    let mut found = false;
    for bucket in buckets.into_iter().flatten() {
        for k in keys {
            if let Some(n) = bucket.get(k).and_then(|v| v.as_u64()) {
                total += n;
                found = true;
            }
        }
    }
    found.then_some(total)
}

// ---------------------------------------------------------------------------
// Tests del parser de contexto. Solo prueban funciones PURAS (sin disco ni
// red); se ejecutan con `cargo test`, no afectan al binario de release.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn usa_context_window_oficial_y_es_consciente_del_modelo_1m() {
        // Ventana de 1M (modelo de contexto extendido): el % debe calcularse
        // contra 1.000.000, no contra 200k.
        let v = json!({
            "context_window": { "total_input_tokens": 240_000, "context_window_size": 1_000_000 }
        });
        let m = metrics_from_capture(&v, 0).expect("debe haber métricas");
        assert_eq!(m.tokens_limit, Some(1_000_000));
        assert_eq!(m.tokens_used, Some(240_000));
        assert_eq!(m.percent_used, Some(24.0)); // 240k / 1M
    }

    #[test]
    fn suma_current_usage_cuando_falta_total_input_tokens() {
        let v = json!({
            "context_window": {
                "context_window_size": 200_000,
                "current_usage": {
                    "input_tokens": 8_000,
                    "cache_creation_input_tokens": 2_000,
                    "cache_read_input_tokens": 30_000,
                    "output_tokens": 999  // la salida NO cuenta para el contexto
                }
            }
        });
        let m = metrics_from_capture(&v, 0).unwrap();
        assert_eq!(m.tokens_used, Some(40_000)); // 8k + 2k + 30k
        assert_eq!(m.percent_used, Some(20.0));
    }

    #[test]
    fn respaldo_flag_exceeds_200k_satura_al_100() {
        let v = json!({ "exceeds_200k_tokens": true });
        let m = metrics_from_capture(&v, 0).unwrap();
        assert_eq!(m.tokens_used, Some(200_000));
        assert_eq!(m.percent_used, Some(100.0));
    }

    #[test]
    fn last_json_value_coge_la_ultima_linea_valida() {
        let raw = "{\"a\":1}\n{\"context_window\":{\"context_window_size\":1000000,\"total_input_tokens\":500000}}\n";
        let v = last_json_value(raw).unwrap();
        assert!(v.get("context_window").is_some());
    }

    // -- Instalación / eliminación de hooks (lógica pura sobre `Value`). Es lo que
    //    toca el `settings.json` del usuario; un fallo aquí rompe sus hooks. --

    const M: &str = "mi-marcador";

    #[test]
    fn append_hook_group_crea_contenedores_y_se_detecta() {
        let mut s = json!({});
        assert!(!event_has_marker(&s, "SessionStart", M));
        append_hook_group(&mut s, "SessionStart", format!("cmd # {M}"));
        assert!(event_has_marker(&s, "SessionStart", M));
        // No falsos positivos en otro evento ni con otro marcador.
        assert!(!event_has_marker(&s, "SessionEnd", M));
        assert!(!event_has_marker(&s, "SessionStart", "otro"));
    }

    #[test]
    fn remove_marked_group_respeta_hooks_ajenos_y_limpia_vacios() {
        // Partimos de un hook AJENO del usuario + el nuestro, bajo el mismo evento.
        let mut s = json!({
            "hooks": { "SessionStart": [
                { "hooks": [ { "type": "command", "command": "echo del-usuario" } ] }
            ] }
        });
        append_hook_group(&mut s, "SessionStart", format!("widget # {M}"));
        assert!(event_has_marker(&s, "SessionStart", M));

        remove_marked_group(&mut s, "SessionStart", M);
        // El nuestro desaparece; el del usuario permanece.
        assert!(!event_has_marker(&s, "SessionStart", M));
        let groups = s["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["hooks"][0]["command"], "echo del-usuario");
    }

    #[test]
    fn remove_marked_group_quita_contenedores_si_quedan_vacios() {
        // Si SOLO estaba el nuestro, al quitarlo no debe dejar `hooks` huérfano.
        let mut s = json!({});
        append_hook_group(&mut s, "SessionEnd", format!("x # {M}"));
        remove_marked_group(&mut s, "SessionEnd", M);
        assert!(s.get("hooks").is_none(), "no debe quedar `hooks` vacío: {s}");
    }

    #[test]
    fn capture_drift_solo_marca_contenedor_conocido_sin_campos() {
        // Sesión recién abierta: sin context_window ni rate_limits → NO es deriva.
        assert!(capture_drift_detail(&json!({ "model": "x", "workspace": {} })).is_none());
        // context_window con sus campos → sin deriva.
        let ok = json!({ "context_window": { "context_window_size": 200000, "total_input_tokens": 10 } });
        assert!(capture_drift_detail(&ok).is_none());
        // context_window PRESENTE pero vacío de campos conocidos → DERIVA.
        let drift = json!({ "context_window": { "algo_nuevo": 1 } });
        assert!(capture_drift_detail(&drift).is_some());
        // rate_limits presente pero sin five_hour/seven_day → DERIVA.
        let drift_rl = json!({ "rate_limits": { "otro": 1 } });
        assert!(capture_drift_detail(&drift_rl).is_some());
    }

    #[test]
    fn existing_foreign_command_distingue_el_nuestro_del_ajeno() {
        // statusLine ajeno → se devuelve para encadenarlo.
        let ajeno = json!({ "statusLine": { "command": "starship prompt" } });
        assert_eq!(existing_foreign_command(&ajeno).as_deref(), Some("starship prompt"));
        // El nuestro (lleva el marcador del puente) → None (no nos auto-envolvemos).
        let nuestro =
            json!({ "statusLine": { "command": format!("node \"x/{BRIDGE_MARKER}.cjs\"") } });
        assert!(existing_foreign_command(&nuestro).is_none());
        // Vacío o ausente → None.
        assert!(existing_foreign_command(&json!({ "statusLine": { "command": "  " } })).is_none());
        assert!(existing_foreign_command(&json!({})).is_none());
    }
}
