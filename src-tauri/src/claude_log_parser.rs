// claude_log_parser.rs
//
// Parser REACTIVO del uso real de Claude Code. La fuente fiable de tokens son
// los transcripts JSONL que Claude Code escribe en `~/.claude/projects/<id>/`.
// Cada mensaje del asistente incluye un objeto `usage` con el desglose exacto
// de tokens del turno.
//
// La métrica que extraemos es el USO DEL CONTEXTO en el último turno:
//     contexto = input_tokens + cache_creation_input_tokens + cache_read_input_tokens
// medido contra la ventana del modelo (200k, o 1M si el uso ya la rebasa). Es un
// dato real (no estimado) y es lo que de verdad indica "cuánto te queda" en la
// sesión activa.
//
// Para no releer archivos de decenas de MB en cada evento, leemos solo la cola
// del transcript más reciente.

use crate::paths;
use crate::{UsageMetrics, SRC_LOGS};
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Ventana de contexto estándar de los modelos Claude.
const CONTEXT_WINDOW: u64 = 200_000;
/// Ventana extendida (modelos con contexto de 1M). El transcript no dice qué
/// ventana usa la sesión; si el uso ya supera los 200k, solo puede ser esta.
const CONTEXT_WINDOW_1M: u64 = 1_000_000;
/// Cuánta cola leer para localizar el último `usage` (suficiente para varios
/// turnos completos sin cargar el archivo entero).
const TAIL_BYTES: u64 = 512 * 1024;
/// Carpeta donde Claude Code guarda los transcripts de SUBAGENTES
/// (`<sesión>/subagents/**/agent-*.jsonl`).
const SUBAGENTS_DIR: &str = "subagents";

/// ¿Es el transcript de una sesión PRINCIPAL? Los de subagentes (Task, workflows)
/// se escriben a la vez que la sesión y, por mtime, "ganaban" la elección del
/// más reciente: el widget mostraba el contexto del subagente, no el tuyo.
fn is_main_transcript(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("jsonl")
        && !path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("agent-"))
        && !path.components().any(|c| c.as_os_str() == SUBAGENTS_DIR)
}

/// Último transcript principal con actividad, según los eventos del watcher.
/// Evita recorrer TODO `~/.claude/projects/` (cientos o miles de ficheros) en
/// cada escritura del turno: el evento ya dice qué fichero cambió.
fn hint() -> &'static Mutex<Option<PathBuf>> {
    static HINT: Mutex<Option<PathBuf>> = Mutex::new(None);
    &HINT
}

/// Lo llama el watcher por cada `.jsonl` que cambia. Devuelve `true` si es el
/// transcript de una sesión principal (y por tanto afecta al contexto).
pub fn note_activity(path: &Path) -> bool {
    if !is_main_transcript(path) {
        return false;
    }
    if let Ok(mut h) = hint().lock() {
        *h = Some(path.to_path_buf());
    }
    true
}

/// Olvida la pista para que la próxima lectura vuelva a escanear (modo de
/// respaldo por sondeo, donde no llegan eventos que la mantengan al día).
pub fn forget_activity() {
    if let Ok(mut h) = hint().lock() {
        *h = None;
    }
}

/// Localiza el transcript principal más reciente: el de la pista del watcher si
/// la hay, o un escaneo completo de `~/.claude/projects/` si no.
///
/// NOTA PARA QUIEN NO CONOZCA EL DETALLE: Claude Code guarda una carpeta de
/// transcripts por cada directorio de trabajo (`cwd`) en el que abres una
/// sesión. Sería ideal mostrar SOLO la sesión que estás mirando, pero este
/// widget es un proceso aparte: NO sabe en qué terminal/cwd estás. Por eso
/// usamos la heurística "el transcript con actividad más reciente" = la
/// sesión que casi siempre tienes delante. Cuando el puente del statusLine
/// está activo (lo recomendado), ese problema desaparece: el JSON del
/// statusLine SÍ corresponde a tu sesión activa, así que esta lectura solo es
/// el respaldo para cuando el puente está apagado.
fn latest_transcript() -> Option<PathBuf> {
    if let Some(p) = hint().lock().ok().and_then(|h| h.clone()) {
        if p.is_file() {
            return Some(p);
        }
    }
    let found = scan_latest(&paths::projects_dir());
    if let (Some(p), Ok(mut h)) = (&found, hint().lock()) {
        *h = Some(p.clone());
    }
    found
}

/// Escaneo completo (arranque, o sin pista): el `.jsonl` principal modificado
/// más recientemente, sin entrar en las carpetas de subagentes.
fn scan_latest(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;

    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != SUBAGENTS_DIR) {
                    stack.push(path);
                }
            } else if is_main_transcript(&path) {
                if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                    if newest.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) {
                        newest = Some((modified, path));
                    }
                }
            }
        }
    }
    newest.map(|(_, p)| p)
}

/// Lee como mucho los últimos `TAIL_BYTES` del archivo, descartando la primera
/// línea (posiblemente parcial) si no leímos desde el inicio.
fn read_tail(path: &PathBuf) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::with_capacity((len - start) as usize);
    f.read_to_end(&mut bytes).ok()?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        if let Some(nl) = text.find('\n') {
            text = text[nl + 1..].to_string();
        }
    }
    Some(text)
}

/// Suma de tokens que ocupan contexto en un objeto `usage`.
fn context_tokens(usage: &Value) -> u64 {
    let f = |k: &str| usage.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    f("input_tokens") + f("cache_creation_input_tokens") + f("cache_read_input_tokens")
}

/// Tokens de contexto del último turno del asistente en la cola de un transcript.
/// Ignora las entradas de cadenas laterales (`isSidechain`). Función PURA.
fn last_context_tokens(tail: &str) -> Option<u64> {
    tail.lines().rev().find_map(|line| {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let value: Value = serde_json::from_str(line).ok()?;
        if value.get("isSidechain").and_then(|v| v.as_bool()) == Some(true) {
            return None;
        }
        let usage = value.get("message")?.get("usage")?;
        let tokens = context_tokens(usage);
        (tokens > 0).then_some(tokens)
    })
}

/// Ventana de contexto que corresponde a un uso dado: la estándar salvo que ya
/// la rebase (entonces la sesión usa un modelo de 1M). Función PURA.
fn window_for(used: u64) -> u64 {
    if used > CONTEXT_WINDOW {
        CONTEXT_WINDOW_1M
    } else {
        CONTEXT_WINDOW
    }
}

/// Lee el transcript más reciente y devuelve el uso real de contexto del
/// último turno del asistente.
pub fn parse_latest() -> Option<UsageMetrics> {
    let path = latest_transcript()?;
    let tail = read_tail(&path)?;
    let used = last_context_tokens(&tail)?;

    let age = paths::file_age_secs(&path);
    Some(UsageMetrics::from_tokens(
        SRC_LOGS,
        "Contexto · Claude Code",
        Some(used),
        Some(window_for(used)),
        age,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distingue_transcripts_de_subagentes() {
        assert!(is_main_transcript(Path::new("/p/proj/4ab1c41e.jsonl")));
        assert!(!is_main_transcript(Path::new("/p/proj/4ab1c41e/subagents/agent-a01.jsonl")));
        assert!(!is_main_transcript(Path::new(
            "/p/proj/4ab1c41e/subagents/workflows/wf_1/agent-a02.jsonl"
        )));
        assert!(!is_main_transcript(Path::new("/p/proj/agent-a03.jsonl")));
        assert!(!is_main_transcript(Path::new("/p/proj/notas.json")));
    }

    #[test]
    fn el_escaneo_ignora_subagentes_aunque_sean_mas_recientes() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("C--proj");
        let sub = proj.join("sess-1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        let main = proj.join("sess-1.jsonl");
        std::fs::write(&main, "{}\n").unwrap();
        // El del subagente se escribe DESPUÉS (más reciente por mtime).
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(sub.join("agent-x.jsonl"), "{}\n").unwrap();
        assert_eq!(scan_latest(tmp.path()), Some(main));
    }

    #[test]
    fn ultimo_turno_ignora_sidechain_y_lineas_rotas() {
        let tail = concat!(
            r#"{"message":{"usage":{"input_tokens":10,"cache_read_input_tokens":90000}}}"#,
            "\n",
            r#"{"isSidechain":true,"message":{"usage":{"input_tokens":5}}}"#,
            "\n",
            "{roto\n"
        );
        assert_eq!(last_context_tokens(tail), Some(90010));
        assert_eq!(last_context_tokens(""), None);
    }

    #[test]
    fn la_ventana_pasa_a_1m_si_el_uso_rebasa_200k() {
        assert_eq!(window_for(150_000), 200_000);
        assert_eq!(window_for(200_000), 200_000);
        assert_eq!(window_for(420_000), 1_000_000);
    }
}
