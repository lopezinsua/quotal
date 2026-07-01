# Arquitectura interna de Quotal

> Documentación para desarrolladores que quieran entender o **extender** Quotal.
> Es fiel al código en `src-tauri/src` (backend Rust) y `src` (frontend JS). El
> plan de evolución vive en [PLAN.md](PLAN.md).

Quotal es un widget Tauri 2. El backend Rust hace todo el trabajo con ficheros y
red; el frontend (JS vanilla, sin framework) solo pinta. Nada bloquea el hilo de
la UI: los watchers y sondeos viven en tareas de `tauri::async_runtime` (Tokio).

## Las dos tuberías de datos

```
Claude Code ──┬── transcripts / statusLine ──> [notify watcher] ──> CONTEXTO ─┐
              └── token OAuth local ──────────> [/usage cada 60s] ──> PLAN ────┤
                                                                               v
                                                             SharedState ──> emit("usage://metrics-updated") ──> UI
```

1. **CONTEXTO** (offline, dirigido por eventos de `notify`): consolida el uso de la
   ventana de tokens de la sesión activa. Fuentes en `providers.rs` (ver abajo).
2. **PLAN** (online, sondeo cada 60s a `/api/oauth/usage`): límites reales de sesión
   (5h) y semana (7d). Con respaldo offline del statusLine y conservación del último
   dato bueno. Vive en `usage_api.rs` y `poller.rs`.

## Punto de extensión: `ContextProvider`

Las fuentes de CONTEXTO implementan un contrato común en `providers.rs`. **Esta es
la costura pensada para desacoplar Quotal de los internals de Claude Code**: el día
que exista una API oficial de uso, será solo otro provider.

```rust
pub trait ContextProvider {
    /// Id ESTABLE: coincide con `UsageMetrics.source` y las clases CSS del front.
    fn id(&self) -> &'static str;      // "hook" | "logs" | "sync"
    /// Métrica de contexto ACTUAL, o None si esta fuente no tiene dato ahora.
    fn read(&self) -> Option<UsageMetrics>;
}
```

Providers actuales (por prioridad, en `providers::all()`):

| id | Provider | Fuente | Módulo |
|----|----------|--------|--------|
| `hook` | `HookProvider` | Captura del statusLine (lo recomendado) | `claude_code_bridge` |
| `logs` | `LogsProvider` | Transcripts `.jsonl` | `claude_log_parser` |
| `sync` | `SyncProvider` | `sync.json` legado | `local_file_watcher` |

### Añadir un provider (p. ej. una API oficial)

1. Implementa `ContextProvider` para un struct nuevo; `read()` devuelve
   `UsageMetrics::from_tokens(id, label, used, limit, age)`.
2. Añádelo a `providers::all()` en su posición de prioridad.
3. Añade su `id` a `SharedState::set_context` y a `Dirty::is_set` (poller), y su
   prioridad a `SharedState::active`.
4. Si se alimenta de un fichero, mapea su ruta en `poller::classify`.

El watcher y el estado ya iteran sobre el registro; no hace falta tocar nada más.

## Contrato de datos (lo que llega al frontend)

Todo viaja serializado como JSON en el evento `usage://metrics-updated` y en el
comando `get_metrics`, con la forma de `MetricsPayload` (`state.rs`):

```jsonc
{
  "active": {                 // UsageMetrics de la fuente de contexto activa
    "source": "hook",         // "hook" | "logs" | "sync" | "none"
    "label": "Contexto · oficial (Claude Code)",
    "tokens_used": 40000,
    "tokens_limit": 200000,
    "percent_used": 20.0,
    "percent_remaining": 80.0,
    "last_seen_secs": 3,
    "stale": false,           // true si last_seen_secs > 300
    "updated_at": "2026-07-01T…"
  },
  "plan": {                   // PlanInfo (usage_api.rs) — límites reales
    "name": "Pro",
    "available": true,
    "error": null,
    "session_percent": 42.0,
    "session_resets_at": "…",
    "session_severity": "normal",   // "normal" | "warning" | "critical"
    "weekly_percent": 12.0,
    "weekly_resets_at": "…",
    "weekly_severity": "normal",
    "fetched_at": "…",
    "source": "online"        // "online" | "statusline" (respaldo offline)
  },
  "schema_warning": ["statusline"],  // fuentes con deriva de formato, o null
  "claude_code_version": "2.1.197"   // versión de CC observada, o null
}
```

La `active()` elige la fuente por prioridad (hook > logs > sync) prefiriendo datos
no-`stale`.

## Red de seguridad ante cambios de Claude Code

Quotal se apoya en formatos NO oficiales (credenciales, statusLine, `/usage`). Para
no romperse en silencio si Anthropic los cambia:

- **`schema_watch.rs`** registra "deriva": cuando un contenedor conocido aparece pero
  le faltan sus campos. Se loguea en `schema_error.log` con la versión de Quotal y de
  Claude Code, y se expone en `MetricsPayload.schema_warning`. La UI lo muestra en un
  banner accionable (comprueba si hay actualización de Quotal).
- **Versión de Claude Code**: se lee del `version` del statusLine y se persiste para
  correlacionar qué versión introdujo un cambio de formato.

Los detectores de deriva son funciones PURAS y testeadas (`*_drift`), para distinguir
"cambió el formato" de "aún no hay dato" (sin login, sesión recién abierta).

## Seguridad y modo solo-lectura

- El token OAuth se **reutiliza** del local de Claude Code; nunca se crea uno propio.
  El refresco es single-flight, con compare-and-swap contra Claude Code antes de
  reescribir el fichero, y `0600` en Unix (`usage_api.rs`).
- **Modo solo-lectura** (`app_config.rs`): con él activo, Quotal no reescribe el token
  ni instala hooks. Se carga al arrancar ANTES del poller. Lo hacen cumplir
  `usage_api::persist_tokens` (no-op) y los comandos `install_*` (rechazan).

## Superficie IPC (comandos `invoke`)

Registrados en `lib.rs`; implementados en `commands.rs`.

| Comando | Devuelve | Qué hace |
|---------|----------|----------|
| `get_metrics` | `MetricsPayload` | Snapshot completo (arranque del front). |
| `get_active_mode` | `{active_source, metrics}` | Fuente de contexto activa. |
| `refresh_plan` | `Result<()>` | Fuerza un sondeo del plan. |
| `get_config` | objeto | Versión, rutas, `read_only`, `bridge_installed`, … |
| `detect_local_sources` | objeto | Qué fuentes locales existen. |
| `install_autostart` / `uninstall_autostart` / `autostart_status` | `Result<()>` / `bool` | Hook `SessionStart`. |
| `install_shutdown` / `uninstall_shutdown` / `shutdown_status` | `Result<()>` / `bool` | Hook `SessionEnd`. |
| `install_statusline_bridge` / `uninstall_statusline_bridge` / `statusline_status` | `Result<()>` / `bool` | Puente del statusLine. |
| `set_read_only` / `read_only_status` | `()` / `bool` | Modo observador. |
| `set_tray_static` / `hide_to_tray` | `()` | Bandeja. |
| `set_bounds` / `animate_bounds` | `Result<()>` | Geometría de la ventana. |
| `update_check` / `update_install` | `UpdateStatus` / `Result<()>` | Auto-actualización. |
| `check_system_deps` | objeto | Libs nativas que faltan (Linux). |

Los `install_*` rechazan en modo solo-lectura.

## Eventos emitidos por el backend

| Evento | Payload | Cuándo |
|--------|---------|--------|
| `usage://metrics-updated` | `MetricsPayload` | Cada consolidación de contexto o plan. |
| `update://available` | `UpdateStatus` | Al arrancar, si hay versión nueva. |
| `window://visibility-changed` | `{visible}` | Al ocultar el widget a la bandeja. |
| `app://will-quit` | — | Antes de un cierre limpio. |

## Mapa de módulos (backend)

| Módulo | Responsabilidad |
|--------|-----------------|
| `lib.rs` | Orquesta: plugins, arranque, `invoke_handler`. |
| `state.rs` | `SharedState`, `UsageMetrics`, `MetricsPayload`. |
| `providers.rs` | Trait `ContextProvider` + registro. |
| `poller.rs` | Watchers `notify` (con respaldo por sondeo) + sondeo del plan. |
| `usage_api.rs` | `/usage`, refresco de token, write-back CAS. |
| `claude_code_bridge.rs` | Puente statusLine + hooks (settings.json). |
| `claude_log_parser.rs` | Parseo de transcripts `.jsonl`. |
| `local_file_watcher.rs` | Parseo de `sync.json`. |
| `schema_watch.rs` | Telemetría de deriva + versión de Claude Code. |
| `app_config.rs` | Config del backend (modo solo-lectura). |
| `paths.rs` | Rutas (Claude Code + widget). |
| `tray.rs` | Icono/menú de bandeja. |
| `commands.rs` | Comandos IPC. |
