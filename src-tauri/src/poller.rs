// poller.rs — Las dos tuberías de datos del backend (ninguna bloquea la UI):
//
//   1. CONTEXTO (offline, dirigido por eventos de archivo `notify`): consolida
//      hook/logs/sync y emite `usage://metrics-updated`.
//   2. LÍMITES DEL PLAN (online, sondeo periódico a `/api/oauth/usage`), con
//      respaldo offline del statusLine y conservación del último dato bueno.

use crate::state::SharedHandle;
use crate::{paths, providers, tray, usage_api, SRC_HOOK, SRC_LOGS, SRC_SYNC};
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Qué fuentes han quedado "sucias" tras una o varias rutas cambiadas. Solo
/// CLASIFICA (no toca disco ni estado), para poder coalescer ráfagas de eventos
/// y hacer una única lectura por fuente.
#[derive(Default)]
struct Dirty {
    hook: bool,
    logs: bool,
    sync: bool,
}

impl Dirty {
    fn any(&self) -> bool {
        self.hook || self.logs || self.sync
    }

    /// ¿Está marcada como sucia la fuente con este `id` de proveedor? Permite que
    /// el bucle de eventos itere el registro de `providers` sin conocer los campos.
    fn is_set(&self, id: &str) -> bool {
        match id {
            SRC_HOOK => self.hook,
            SRC_LOGS => self.logs,
            SRC_SYNC => self.sync,
            _ => false,
        }
    }
}

/// Marca la fuente afectada por una ruta. Comparamos por nombre de archivo /
/// extensión para ser robustos ante diferencias de canonicalización entre SO.
fn classify(path: &Path, d: &mut Dirty) {
    if path.ends_with("statusline-capture.json") {
        d.hook = true;
    } else if path.ends_with("sync.json") {
        d.sync = true;
    } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
        d.logs = true;
    }
}

/// Pobla todas las fuentes de una sola pasada (escaneo inicial al arrancar y en el
/// respaldo por sondeo), iterando el registro de proveedores de contexto.
fn refresh_all(shared: &SharedHandle) {
    if let Ok(mut state) = shared.lock() {
        for p in providers::all() {
            let m = p.read();
            state.set_context(p.id(), m);
        }
    }
}

fn emit_active(app: &AppHandle, shared: &SharedHandle) {
    let payload = shared.lock().ok().map(|s| s.payload());
    if let Some(payload) = payload {
        let _ = app.emit("usage://metrics-updated", payload);
    }
}

/// Cada cuánto reparsear las fuentes en el modo de RESPALDO por sondeo (cuando
/// `notify` no está disponible). Más frecuente que el sondeo del plan porque el
/// contexto cambia en tiempo real; sigue siendo barato (solo lee colas de ficheros).
const POLL_FALLBACK: Duration = Duration::from_secs(5);

/// Bucle de respaldo por SONDEO para cuando `notify` no puede vigilar los ficheros
/// (no se pudo crear el watcher, o el SO rechazó registrar los watches — p. ej. al
/// agotar los límites de inotify en Linux). Menos eficiente que los eventos, pero
/// garantiza que la UI SIGA actualizándose en vez de congelarse en silencio.
async fn poll_loop(app: AppHandle, shared: SharedHandle) {
    log::warn!(
        "[watcher] respaldo por sondeo activo (cada {}s): notify no disponible",
        POLL_FALLBACK.as_secs()
    );
    loop {
        tokio::time::sleep(POLL_FALLBACK).await;
        refresh_all(&shared);
        emit_active(&app, &shared);
    }
}

/// Monta los watchers de `notify` y la tarea consumidora. No bloquea el hilo
/// principal: todo corre dentro de `tauri::async_runtime`.
pub(crate) fn spawn_watchers(app: AppHandle, shared: SharedHandle) {
    let _ = paths::ensure_widget_dir();

    // Escaneo + emisión inicial para que la UI no arranque vacía.
    refresh_all(&shared);
    emit_active(&app, &shared);

    tauri::async_runtime::spawn(async move {
        use notify::{RecursiveMode, Watcher};

        // Canal puente: la callback de notify (hilo del SO) hace `blocking_send`
        // y la tarea async consume con `recv().await`.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<notify::Event>(64);

        let mut watcher =
            match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    // El emisor vive en el hilo de notify (no en el runtime tokio),
                    // así que `blocking_send` es seguro aquí.
                    let _ = tx.blocking_send(event);
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    // Sin watcher no hay eventos: caemos al respaldo por sondeo en
                    // vez de quedarnos sin actualizar el contexto para siempre.
                    log::error!("[watcher] no se pudo crear el watcher de notify: {e}");
                    poll_loop(app, shared).await;
                    return;
                }
            };

        // Vigilamos el directorio del widget (sync.json + captura del hook) y,
        // si existe, el de historial. NonRecursive basta para el primero;
        // Recursive para historial por si hay subcarpetas. Registramos si AL MENOS
        // uno se pudo vigilar; si ninguno (p. ej. límites de inotify agotados),
        // caemos al respaldo por sondeo.
        let mut watched = false;
        match watcher.watch(&paths::widget_dir(), RecursiveMode::NonRecursive) {
            Ok(()) => watched = true,
            Err(e) => log::warn!("[watcher] widget_dir: {e}"),
        }
        if paths::projects_dir().exists() {
            match watcher.watch(&paths::projects_dir(), RecursiveMode::Recursive) {
                Ok(()) => watched = true,
                Err(e) => log::warn!("[watcher] projects_dir: {e}"),
            }
        }
        if !watched {
            log::error!("[watcher] ningún directorio vigilable; respaldo por sondeo");
            poll_loop(app, shared).await;
            return;
        }

        // `watcher` debe seguir vivo mientras dure el bucle: lo retenemos aquí.
        let _keep_alive = &watcher;

        // Coalescemos ráfagas: durante la generación de un turno, Claude Code
        // escribe en el `.jsonl` muchas veces por segundo. En vez de re-parsear
        // y re emitir por CADA escritura, agrupamos los eventos de una ventana
        // corta y hacemos UNA lectura por fuente y UNA emisión.
        const DEBOUNCE: Duration = Duration::from_millis(250);
        while let Some(first) = rx.recv().await {
            let mut dirty = Dirty::default();
            for path in &first.paths {
                classify(path, &mut dirty);
            }
            // Drena el resto de la ráfaga hasta agotar la ventana de debounce.
            let timer = tokio::time::sleep(DEBOUNCE);
            tokio::pin!(timer);
            loop {
                tokio::select! {
                    _ = &mut timer => break,
                    maybe = rx.recv() => match maybe {
                        Some(ev) => {
                            for path in &ev.paths {
                                classify(path, &mut dirty);
                            }
                        }
                        None => break,
                    },
                }
            }
            if !dirty.any() {
                continue;
            }
            // Parseamos FUERA del lock (es IO de disco): leemos solo las fuentes
            // SUCIAS del registro y luego asignamos rápido bajo el lock.
            let updates: Vec<_> = providers::all()
                .into_iter()
                .filter(|p| dirty.is_set(p.id()))
                .map(|p| (p.id(), p.read()))
                .collect();
            if let Ok(mut state) = shared.lock() {
                for (id, m) in updates {
                    state.set_context(id, m);
                }
            }
            emit_active(&app, &shared);
        }
    });
}

/// Cada cuánto refrescamos los límites del plan cuando todo va bien. El endpoint
/// `/api/oauth/usage` está fuertemente limitado (429 si se abusa); 60s es
/// "constante" de sobra para datos que cambian en minutos.
pub(crate) const PLAN_POLL: Duration = Duration::from_secs(60);
/// Espera mayor tras un fallo (p. ej. 429) para no empeorar el rate-limit.
const PLAN_POLL_BACKOFF: Duration = Duration::from_secs(180);

/// Bucle que sondea `/api/oauth/usage` periódicamente y empuja los datos reales
/// del plan al frontend. Corre en `tauri::async_runtime` (tokio), sin bloquear.
pub(crate) fn spawn_plan_poller(app: AppHandle, shared: SharedHandle) {
    // Muestra al instante el último plan cacheado mientras llega el primer sondeo.
    if let Some(cached) = usage_api::load_cache() {
        apply_plan(&app, &shared, cached);
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let info = usage_api::fetch().await;
            let ok = info.available;
            if !ok {
                log::warn!(
                    "sondeo de plan sin datos en vivo: {}",
                    info.error.as_deref().unwrap_or("motivo desconocido")
                );
            }
            // El respaldo del statusLine y la conservación del último dato bueno
            // los decide `apply_plan` (así un 429 transitorio NO parpadea a "Sin
            // conexión" si ya teníamos datos válidos).
            apply_plan(&app, &shared, info);
            tokio::time::sleep(if ok { PLAN_POLL } else { PLAN_POLL_BACKOFF }).await;
        }
    });
}

/// Guarda el plan en el estado, actualiza el icono/tooltip de la bandeja según
/// la severidad y empuja el payload al frontend.
///
/// Política ante un fetch FALLIDO (429/red), de menos a más drástica:
///   1. Si ya teníamos un plan bueno -> lo CONSERVAMOS (la frescura envejece;
///      nada de parpadear a "Sin conexión" por un 429 transitorio).
///   2. Si no teníamos nada -> respaldo OFICIAL del statusLine (mismos límites,
///      offline) si está disponible.
///   3. Si tampoco hay statusLine → mostramos el error (honestos: sin datos).
pub(crate) fn apply_plan(app: &AppHandle, shared: &SharedHandle, info: usage_api::PlanInfo) {
    // Si el fetch falló, prepara el respaldo del statusLine ANTES de tomar el
    // lock (evita I/O de disco dentro del mutex). Solo se usará si hace falta.
    let fallback = if info.available { None } else { usage_api::plan_from_statusline() };

    let (severity, remaining, tooltip, to_cache) = {
        let mut guard = match shared.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        if info.available {
            guard.plan = info; // dato bueno (normalmente online)
        } else if !guard.plan.available {
            // No había nada usable: statusLine oficial o, en su defecto, el error.
            guard.plan = fallback.unwrap_or(info);
        }
        // (si falló pero YA teníamos un plan bueno, no tocamos guard.plan)
        let to_cache = if guard.plan.available { Some(guard.plan.clone()) } else { None };
        (
            guard.plan.session_status(),
            guard.plan.session_remaining(),
            guard.plan.tray_tooltip(),
            to_cache,
        )
    };
    tray::set_gauge(app, remaining, severity, &tooltip);
    emit_active(app, shared);
    // Persistimos el último plan bueno FUERA del lock (IO de disco).
    if let Some(p) = to_cache {
        usage_api::save_cache(&p);
    }
}
