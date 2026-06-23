// claude_log_parser.rs
//
// Parser REACTIVO del uso real de Claude Code. La fuente fiable de tokens son
// los transcripts JSONL que Claude Code escribe en `~/.claude/projects/<id>/`.
// Cada mensaje del asistente incluye un objeto `usage` con el desglose exacto
// de tokens del turno.
//
// La métrica que extraemos es el USO DEL CONTEXTO en el último turno:
//     contexto = input_tokens + cache_creation_input_tokens + cache_read_input_tokens
// medido contra la ventana de 200k. Es un dato real (no estimado) y es lo que
// de verdad indica "cuánto te queda" en la sesión activa.
//
// Para no releer archivos de decenas de MB en cada evento, leemos solo la cola
// del transcript más reciente.

use crate::paths;
use crate::{UsageMetrics, SRC_LOGS};
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Ventana de contexto de referencia (modelos Claude estándar).
const CONTEXT_WINDOW: u64 = 200_000;
/// Cuánta cola leer para localizar el último `usage` (suficiente para varios
/// turnos completos sin cargar el archivo entero).
const TAIL_BYTES: u64 = 512 * 1024;

/// Localiza el transcript `.jsonl` modificado más recientemente bajo
/// `~/.claude/projects/` (recursivo, un par de niveles de profundidad).
///
/// NOTA PARA QUIEN NO CONOZCA EL DETALLE: Claude Code guarda una carpeta de
/// transcripts por cada directorio de trabajo (`cwd`) en el que abres una
/// sesión. Sería ideal mostrar SOLO la sesión que estás mirando, pero este
/// widget es un proceso aparte: NO sabe en qué terminal/cwd estás. Por eso
/// usamos la heurística "el transcript modificado más recientemente" = la
/// sesión con actividad más reciente, que casi siempre es la que tienes
/// delante. Cuando el puente del statusLine está activo (lo recomendado), ese
/// problema desaparece: el JSON del statusLine SÍ corresponde a tu sesión
/// activa, así que esta lectura solo es el respaldo para cuando el puente
/// está apagado.
fn latest_transcript() -> Option<PathBuf> {
    let mut stack = vec![paths::projects_dir()];
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;

    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
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

/// Escanea el transcript más reciente y devuelve el uso real de contexto del
/// último turno del asistente.
pub fn parse_latest() -> Option<UsageMetrics> {
    let path = latest_transcript()?;
    let tail = read_tail(&path)?;

    // Recorremos de atrás hacia delante buscando el último `message.usage`.
    let used = tail.lines().rev().find_map(|line| {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let value: Value = serde_json::from_str(line).ok()?;
        let usage = value.get("message")?.get("usage")?;
        let tokens = context_tokens(usage);
        (tokens > 0).then_some(tokens)
    })?;

    let age = paths::file_age_secs(&path);
    Some(UsageMetrics::from_tokens(
        SRC_LOGS,
        "Contexto · Claude Code",
        Some(used),
        Some(CONTEXT_WINDOW),
        age,
    ))
}
