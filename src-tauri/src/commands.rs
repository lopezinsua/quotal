// commands.rs — Comandos IPC expuestos al frontend (`invoke(...)`). Cada uno es
// fino: delega en `usage_api`, `claude_code_bridge`, `poller` o el estado.

use crate::poller;
use crate::state::{MetricsPayload, SharedHandle, UsageMetrics};
use crate::{claude_code_bridge, paths, usage_api};
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct ActiveMode {
    active_source: String,
    metrics: UsageMetrics,
}

#[tauri::command]
pub fn get_active_mode(state: State<'_, SharedHandle>) -> ActiveMode {
    let metrics = state.lock().map(|s| s.active()).unwrap_or_else(|_| UsageMetrics::none());
    ActiveMode { active_source: metrics.source.clone(), metrics }
}

/// Snapshot completo (gauge activo + totales de sesión/semana) para el arranque
/// del frontend, evitando la carrera con la emisión inicial.
#[tauri::command]
pub fn get_metrics(state: State<'_, SharedHandle>) -> MetricsPayload {
    state.lock().map(|s| s.payload()).unwrap_or_else(|_| MetricsPayload {
        active: UsageMetrics::none(),
        plan: usage_api::PlanInfo::default(),
        schema_warning: None,
    })
}

#[derive(Serialize)]
pub struct LocalSources {
    claude_binary: Option<String>,
    projects_dir_exists: bool,
    sync_file_exists: bool,
    capture_file_exists: bool,
    bridge_installed: bool,
}

#[tauri::command]
pub fn detect_local_sources() -> LocalSources {
    LocalSources {
        claude_binary: claude_code_bridge::detect_claude_binary(),
        projects_dir_exists: paths::projects_dir().exists(),
        sync_file_exists: paths::sync_path().exists(),
        capture_file_exists: paths::capture_path().exists(),
        bridge_installed: claude_code_bridge::is_bridge_installed(),
    }
}

/// Fuerza un refresco inmediato de los límites del plan (botón de la UI).
#[tauri::command]
pub async fn refresh_plan(app: AppHandle, state: State<'_, SharedHandle>) -> Result<(), String> {
    let info = usage_api::fetch().await;
    poller::apply_plan(&app, state.inner(), info);
    Ok(())
}

/// Instala el auto-arranque: hook `SessionStart` que abre el widget al iniciar
/// Claude Code en la terminal.
#[tauri::command]
pub fn install_autostart() -> Result<(), String> {
    claude_code_bridge::install_autostart_hook()
}

/// Quita el auto-arranque con Claude Code.
#[tauri::command]
pub fn uninstall_autostart() -> Result<(), String> {
    claude_code_bridge::uninstall_autostart_hook()
}

/// Está activo el auto-arranque con Claude Code?
#[tauri::command]
pub fn autostart_status() -> bool {
    claude_code_bridge::is_autostart_installed()
}

/// Instala el auto-cierre: hook `SessionEnd` que cierra el widget al salir de
/// Claude Code en la terminal.
#[tauri::command]
pub fn install_shutdown() -> Result<(), String> {
    claude_code_bridge::install_shutdown_hook()
}

/// Quita el auto-cierre con Claude Code.
#[tauri::command]
pub fn uninstall_shutdown() -> Result<(), String> {
    claude_code_bridge::uninstall_shutdown_hook()
}

/// Está activo el auto-cierre con Claude Code?
#[tauri::command]
pub fn shutdown_status() -> bool {
    claude_code_bridge::is_shutdown_installed()
}

/// Está activo el puente statusLine (captura del contexto oficial)?
#[tauri::command]
pub fn statusline_status() -> bool {
    claude_code_bridge::is_bridge_installed()
}

#[tauri::command]
pub fn install_statusline_bridge() -> Result<(), String> {
    claude_code_bridge::install_statusline_bridge()
}

#[tauri::command]
pub fn uninstall_statusline_bridge() -> Result<(), String> {
    claude_code_bridge::uninstall_statusline_bridge()
}

/// Activa/desactiva el color fijo del icono de bandeja y lo redibuja al instante
/// con el plan actual (sin esperar al próximo sondeo).
#[tauri::command]
pub fn set_tray_static(app: AppHandle, state: State<'_, SharedHandle>, enabled: bool) {
    crate::tray::set_tray_static(enabled);
    let (remaining, sev, tooltip) = match state.lock() {
        Ok(g) => (g.plan.session_remaining(), g.plan.session_status(), g.plan.tray_tooltip()),
        Err(_) => return,
    };
    crate::tray::set_gauge(&app, remaining, sev, &tooltip);
}

/// Aplica posición Y tamaño (físicos) JUNTOS para que el cambio de modo
/// píldora↔completo sea fluido, sin el paso intermedio "redimensiona y luego
/// salta".
///
/// En Windows usa `SetWindowPos`, que mueve y redimensiona en UNA operación
/// atómica del SO: la esquina anclada no se mueve en ningún frame intermedio. En
/// otras plataformas cae a dos pasos ordenados (al crecer posiciona→agranda; al
/// encoger reduce→mueve) para que el intermedio sea siempre una ventana pequeña.
#[tauri::command]
pub fn set_bounds(app: AppHandle, x: i32, y: i32, w: u32, h: u32) -> Result<(), String> {
    use tauri::Manager;
    let win = app.get_webview_window("main").ok_or_else(|| "sin ventana main".to_string())?;

    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
        let hwnd = win.hwnd().map_err(|e| e.to_string())?;
        // SAFETY: hwnd es válido (lo da Tauri); SetWindowPos sin cambiar Z-order ni
        // foco. Atómico: posición + tamaño en una sola llamada del compositor.
        unsafe {
            SetWindowPos(hwnd, None, x, y, w as i32, h as i32, SWP_NOZORDER | SWP_NOACTIVATE)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        let pos = PhysicalPosition::new(x, y);
        let size = PhysicalSize::new(w, h);
        let growing = win
            .outer_size()
            .map(|c| (w as u64) * (h as u64) > (c.width as u64) * (c.height as u64))
            .unwrap_or(true);
        if growing {
            win.set_position(pos).map_err(|e| e.to_string())?;
            win.set_size(size).map_err(|e| e.to_string())?;
        } else {
            win.set_size(size).map_err(|e| e.to_string())?;
            win.set_position(pos).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Generación de animación: cada nueva animación incrementa el contador y el
/// hilo en curso se detiene si deja de ser el actual (cancela animaciones
/// superpuestas al alternar modos rápido, evitando que dos peleen por el tamaño).
/// Solo se usa en la rama de animación de Windows; en otras plataformas no se
/// compila (evita un aviso de `dead_code` con clippy `-D warnings`).
#[cfg(windows)]
static ANIM_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Anima la ventana desde su posición/tamaño ACTUAL hasta el destino (físicos) en
/// `ms`, con easing (ease-out exponencial, sin sobrepaso). En Windows mueve+redimensiona con
/// `SetWindowPos` en un hilo a ~120 fps (suave, sin IPC por frame). En otras
/// plataformas aplica el destino de golpe (sin animar).
#[tauri::command]
pub fn animate_bounds(
    app: AppHandle,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    ms: u64,
) -> Result<(), String> {
    use tauri::Manager;
    let win = app.get_webview_window("main").ok_or_else(|| "sin ventana main".to_string())?;

    #[cfg(windows)]
    {
        use std::sync::atomic::Ordering;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
        let from_pos = win.outer_position().map_err(|e| e.to_string())?;
        let from_size = win.outer_size().map_err(|e| e.to_string())?;
        let hwnd = win.hwnd().map_err(|e| e.to_string())?;
        // HWND no es Send: pasamos el puntero como isize y lo reconstruimos dentro.
        let hwnd_raw = hwnd.0 as isize;
        let (fx, fy) = (from_pos.x as f64, from_pos.y as f64);
        let (fw, fh) = (from_size.width as f64, from_size.height as f64);
        let (tx, ty, tw, th) = (x as f64, y as f64, w as f64, h as f64);
        let my_gen = ANIM_GEN.fetch_add(1, Ordering::SeqCst) + 1;
        std::thread::spawn(move || {
            use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
            let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
            // Sube la resolución del temporizador a 1 ms durante la animación. Sin
            // esto, `sleep` en Windows redondea a ~15 ms y los frames salen
            // irregulares (se percibe a tirones). Se restaura al terminar.
            unsafe { timeBeginPeriod(1) };
            let total_ms = ms.max(1) as f64;
            let start = std::time::Instant::now();
            loop {
                if ANIM_GEN.load(Ordering::SeqCst) != my_gen {
                    break; // animación superada por otra
                }
                // Progreso por TIEMPO REAL transcurrido, no por número de frame:
                // aunque un frame se retrase, la posición sigue siendo la correcta
                // para ese instante → movimiento uniforme, sin acumular deriva.
                let t = (start.elapsed().as_secs_f64() * 1000.0 / total_ms).min(1.0);
                // Ease-OUT exponencial: arranca rápido y aterriza con una
                // desaceleración muy suave, SIN sobrepaso ni rebote. Es la misma
                // sensación que las animaciones de ventana del SO: responde al
                // instante y se asienta limpio. La misma curva (su aprox. en
                // cubic-bezier) se usa en el CSS del contenido y del border-radius,
                // así ventana, crossfade y esquina se mueven acompasados.
                let e = if t >= 1.0 { 1.0 } else { 1.0 - 2f64.powf(-10.0 * t) };
                let cx = (fx + (tx - fx) * e).round() as i32;
                let cy = (fy + (ty - fy) * e).round() as i32;
                let cw = ((fw + (tw - fw) * e).round() as i32).max(1);
                let ch = ((fh + (th - fh) * e).round() as i32).max(1);
                unsafe {
                    let _ = SetWindowPos(hwnd, None, cx, cy, cw, ch, SWP_NOZORDER | SWP_NOACTIVATE);
                }
                if t >= 1.0 {
                    break; // llegó al destino exacto
                }
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            unsafe { timeEndPeriod(1) };
        });
        Ok(())
    }

    #[cfg(not(windows))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        let _ = ms;
        win.set_size(PhysicalSize::new(w, h)).map_err(|e| e.to_string())?;
        win.set_position(PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[tauri::command]
pub fn get_config(state: State<'_, SharedHandle>) -> serde_json::Value {
    let active = state.lock().map(|s| s.active()).unwrap_or_else(|_| UsageMetrics::none());
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        // Híbrido: contexto offline (notify) + límites del plan online (/usage).
        "context_source": "offline",
        "plan_source": "online:/api/oauth/usage",
        "plan_poll_secs": poller::PLAN_POLL.as_secs(),
        "stale_after_secs": crate::STALE_AFTER_SECS,
        "paths": {
            "claude_dir": paths::claude_dir().to_string_lossy(),
            "projects_dir": paths::projects_dir().to_string_lossy(),
            "widget_dir": paths::widget_dir().to_string_lossy(),
            "sync_file": paths::sync_path().to_string_lossy(),
        },
        "bridge_installed": claude_code_bridge::is_bridge_installed(),
        "active_mode": active.source,
        "active_metrics": active,
    })
}

// ---------------------------------------------------------------------------
// Auto-actualización: comprobar e instalar bajo demanda (la UI avisa con botón).
// ---------------------------------------------------------------------------

/// Estado de actualización que viaja al frontend (evento `update://available` y
/// respuesta de `update_check`).
#[derive(Serialize, Clone)]
pub struct UpdateStatus {
    /// Hay una versión más reciente disponible.
    pub available: bool,
    /// Versión disponible (si la hay).
    pub version: Option<String>,
    /// Versión instalada actualmente.
    pub current: String,
    /// Notas de la versión, si el manifiesto las incluye.
    pub notes: Option<String>,
    /// Mensaje de error si la comprobación falló (sin red, en dev, etc.).
    pub error: Option<String>,
}

impl UpdateStatus {
    fn unavailable(error: Option<String>) -> Self {
        UpdateStatus {
            available: false,
            version: None,
            current: env!("CARGO_PKG_VERSION").to_string(),
            notes: None,
            error,
        }
    }
}

/// Consulta el endpoint del updater SIN instalar nada: solo informa. La usan
/// tanto el arranque (para emitir el aviso) como el botón "Buscar
/// actualizaciones" de los ajustes.
pub async fn fetch_update_status(app: &AppHandle) -> UpdateStatus {
    use tauri_plugin_updater::UpdaterExt;
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => return UpdateStatus::unavailable(Some(e.to_string())),
    };
    match updater.check().await {
        Ok(Some(update)) => UpdateStatus {
            available: true,
            version: Some(update.version.clone()),
            current: update.current_version.clone(),
            notes: update.body.clone(),
            error: None,
        },
        Ok(None) => UpdateStatus::unavailable(None),
        Err(e) => UpdateStatus::unavailable(Some(e.to_string())),
    }
}

/// Comprobación manual (botón de ajustes). Devuelve el estado al frontend.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateStatus {
    fetch_update_status(&app).await
}

/// Descarga la actualización, VERIFICA su firma minisign (clave pública en
/// tauri.conf.json), la instala y reinicia la app para aplicarla. Re-comprueba
/// para obtener el artefacto fresco. Devuelve error si algo falla.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no hay actualización disponible".to_string())?;
    update.download_and_install(|_chunk, _total| {}, || {}).await.map_err(|e| e.to_string())?;
    app.restart()
}

// ---------------------------------------------------------------------------
// Dependencias del sistema (Linux): librerías nativas que el widget necesita
// para funcionar al 100% (render WebKit, icono de bandeja, iconos SVG). En
// Windows/macOS no aplica (el runtime va incrustado o es del sistema): devuelve
// vacío y la UI no muestra nada.
// ---------------------------------------------------------------------------

/// Una dependencia nativa que falta, con el paquete a instalar en el gestor
/// detectado.
#[derive(Serialize, Clone)]
pub struct MissingDep {
    /// Nombre legible (p. ej. "WebKitGTK 4.1").
    pub name: String,
    /// Paquete a instalar en el gestor detectado (p. ej. "libwebkit2gtk-4.1-0").
    pub package: String,
}

/// Informe de dependencias: las que faltan y un comando para instalarlas todas.
#[derive(Serialize, Clone, Default)]
pub struct DepsReport {
    pub missing: Vec<MissingDep>,
    /// Comando sugerido (`sudo apt install …`) según el gestor detectado.
    pub install_hint: Option<String>,
}

/// Comando: comprueba qué dependencias nativas faltan. La UI lo llama al
/// arrancar y muestra el aviso solo si la lista no está vacía.
#[tauri::command]
pub fn check_system_deps() -> DepsReport {
    scan_system_deps()
}

#[cfg(target_os = "linux")]
pub fn scan_system_deps() -> DepsReport {
    // (nombre legible, soname a buscar, [paquete apt, dnf, pacman]).
    let known: &[(&str, &str, [&str; 3])] = &[
        (
            "WebKitGTK 4.1",
            "libwebkit2gtk-4.1.so",
            ["libwebkit2gtk-4.1-0", "webkit2gtk4.1", "webkit2gtk-4.1"],
        ),
        (
            "Ayatana AppIndicator",
            "libayatana-appindicator3.so",
            [
                "libayatana-appindicator3-1",
                "libayatana-appindicator-gtk3",
                "libayatana-appindicator",
            ],
        ),
        ("librsvg", "librsvg-2.so", ["librsvg2-2", "librsvg2", "librsvg"]),
    ];

    // Una sola pasada de `ldconfig -p` lista todas las libs registradas.
    let listing = std::process::Command::new("ldconfig")
        .arg("-p")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    // Gestor de paquetes -> columna de nombres + prefijo del comando.
    let (idx, cmd) = if which("apt-get") {
        (0, "sudo apt install")
    } else if which("dnf") {
        (1, "sudo dnf install")
    } else if which("pacman") {
        (2, "sudo pacman -S")
    } else {
        (0, "sudo apt install")
    };

    let mut missing = Vec::new();
    let mut pkgs = Vec::new();
    for (name, soname, pkg) in known {
        if !listing.contains(soname) {
            missing.push(MissingDep { name: name.to_string(), package: pkg[idx].to_string() });
            pkgs.push(pkg[idx]);
        }
    }
    let install_hint = (!pkgs.is_empty()).then(|| format!("{} {}", cmd, pkgs.join(" ")));
    DepsReport { missing, install_hint }
}

/// En el resto de plataformas no hay dependencias nativas que comprobar.
#[cfg(not(target_os = "linux"))]
pub fn scan_system_deps() -> DepsReport {
    DepsReport::default()
}

/// ¿Está disponible este ejecutable? (detección de gestor de paquetes).
#[cfg(target_os = "linux")]
fn which(bin: &str) -> bool {
    std::process::Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
