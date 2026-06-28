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
/// `ms`, con easing (ease-out cúbico). En Windows mueve+redimensiona con
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
            let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
            let frames = ((ms as f64 / 8.0).round() as i64).max(1);
            for i in 1..=frames {
                if ANIM_GEN.load(Ordering::SeqCst) != my_gen {
                    return; // animación superada por otra
                }
                let t = i as f64 / frames as f64;
                let e = 1.0 - (1.0 - t).powi(3); // ease-out cúbico
                let cx = (fx + (tx - fx) * e).round() as i32;
                let cy = (fy + (ty - fy) * e).round() as i32;
                let cw = ((fw + (tw - fw) * e).round() as i32).max(1);
                let ch = ((fh + (th - fh) * e).round() as i32).max(1);
                unsafe {
                    let _ = SetWindowPos(hwnd, None, cx, cy, cw, ch, SWP_NOZORDER | SWP_NOACTIVATE);
                }
                std::thread::sleep(std::time::Duration::from_millis(8));
            }
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
