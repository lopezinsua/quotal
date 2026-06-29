// tray.rs
//
// Icono y menú nativo de bandeja. El texto del primer ítem alterna entre
// "Mostrar widget" / "Ocultar widget" según el estado real de la ventana,
// para no desincronizarse nunca. "Salir" es la única vía de cierre limpio.

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager, Wry,
};

/// Si está activo, el icono de bandeja usa SIEMPRE el color "normal" (de marca),
/// sin cambiar por severidad. Lo conmuta el usuario desde Ajustes (comando IPC).
static TRAY_STATIC: AtomicBool = AtomicBool::new(false);

/// Activa/desactiva el color fijo del icono de bandeja.
pub fn set_tray_static(on: bool) {
    TRAY_STATIC.store(on, Ordering::Relaxed);
}

// Icono inicial (antes del primer sondeo): PNG embebido. A partir de ahí, el
// icono se DIBUJA dinámicamente con `gauge_icon` según el uso de la sesión.
const ICON_NORMAL: &[u8] = include_bytes!("../icons/tray-normal.png");

/// Guardamos el ítem de toggle para poder actualizar su texto al vuelo.
pub struct TrayState {
    toggle: MenuItem<Wry>,
}

const SHOW_TEXT: &str = "Mostrar widget";
const HIDE_TEXT: &str = "Ocultar widget";

pub fn create_tray(app: &App<Wry>) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", HIDE_TEXT, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &quit])?;

    app.manage(TrayState { toggle: toggle.clone() });

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("Quotal")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => toggle_window(app),
            "quit" => quit_clean(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Click izquierdo en el icono también alterna la visibilidad.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        });

    // Icono inicial: el PNG de bandeja embebido (es lo correcto: la bandeja
    // muestra su propio icono, no el de la ventana). Si por lo que sea no
    // decodificara, caemos al icono de ventana; y si tampoco hubiera, la bandeja
    // se construye SIN icono en vez de entrar en pánico al arrancar.
    if let Ok(img) = Image::from_bytes(ICON_NORMAL) {
        builder = builder.icon(img);
    } else if let Some(default) = app.default_window_icon() {
        builder = builder.icon(default.clone());
    }
    builder.build(app)?;

    Ok(())
}

/// Color RGB por severidad ("normal" | "warning" | "critical"). En normal usa el
/// verde-teal de la marca (el del logo).
fn severity_rgb(severity: &str) -> [u8; 3] {
    match severity {
        "critical" => [255, 80, 70], // rojo
        "warning" => [255, 175, 45], // ámbar
        _ => [80, 208, 160],         // verde-teal (marca / logo)
    }
}

/// Dibuja el icono dinámico de la bandeja con la estética del LOGO: un gauge
/// ABIERTO (anillo de 270° con el hueco abajo) + un punto central (el "hub"),
/// minimalista. El sector de COLOR (severidad/marca) = fracción que QUEDA de la
/// sesión (0..1), llenando desde abajo-izquierda en horario; el resto es un sector
/// GRIS sutil (lo consumido), para que el nivel se lea siempre. `remaining = None`
/// -> todo el arco en gris (sin datos). 32x32 px, supermuestreo 4x4. (RGBA, lado).
fn gauge_icon(remaining: Option<f64>, color: [u8; 3]) -> (Vec<u8>, u32) {
    const S: i32 = 32;
    const SS: i32 = 4; // supermuestreo por eje (4x4 = 16 muestras)
    let c = S as f64 / 2.0;
    let rc = 12.5_f64; // radio de la línea media del anillo
    let half = 2.3_f64; // media anchura del trazo (~4.6px): fino y limpio
    let dot_r = 2.3_f64; // punto central (hub del logo)
    let two_pi = std::f64::consts::PI * 2.0;
    let sweep = two_pi * 0.75; // 270° (gauge abierto por abajo)
    let start = std::f64::consts::PI * 1.25; // 225°: arranca abajo-izquierda
    let frac = remaining.unwrap_or(0.0).clamp(0.0, 1.0);
    let fill = sweep * frac;
    let has = remaining.is_some();
    let gray: [f64; 3] = [92.0, 96.0, 108.0]; // sector consumido (sutil)
    let col: [f64; 3] = [color[0] as f64, color[1] as f64, color[2] as f64];
    let white: [f64; 3] = [238.0, 242.0, 246.0]; // punto central
    let step = 1.0 / (SS * SS) as f64;

    let mut buf = vec![0u8; (S * S * 4) as usize];
    for y in 0..S {
        for x in 0..S {
            let (mut cov_col, mut cov_gray, mut cov_dot) = (0.0_f64, 0.0_f64, 0.0_f64);
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f64 + (sx as f64 + 0.5) / SS as f64;
                    let py = y as f64 + (sy as f64 + 0.5) / SS as f64;
                    let dx = px - c;
                    let dy = py - c;
                    let r = (dx * dx + dy * dy).sqrt();
                    // Punto central (hub).
                    if r <= dot_r {
                        cov_dot += step;
                        continue;
                    }
                    // Trazo del anillo (banda alrededor de la línea media).
                    if (r - rc).abs() > half {
                        continue;
                    }
                    // Ángulo desde arriba (12 en punto), horario, 0..2π.
                    let mut a = dx.atan2(-dy);
                    if a < 0.0 {
                        a += two_pi;
                    }
                    // Posición a lo largo del barrido del gauge (0..sweep); fuera = hueco.
                    let t = (a - start).rem_euclid(two_pi);
                    if t > sweep {
                        continue;
                    }
                    if has && t <= fill {
                        cov_col += step;
                    } else {
                        cov_gray += step;
                    }
                }
            }
            let total = cov_col + cov_gray + cov_dot;
            if total <= 0.0 {
                continue;
            }
            let idx = ((y * S + x) * 4) as usize;
            buf[idx] = ((col[0] * cov_col + gray[0] * cov_gray + white[0] * cov_dot) / total)
                .round() as u8;
            buf[idx + 1] = ((col[1] * cov_col + gray[1] * cov_gray + white[1] * cov_dot) / total)
                .round() as u8;
            buf[idx + 2] = ((col[2] * cov_col + gray[2] * cov_gray + white[2] * cov_dot) / total)
                .round() as u8;
            buf[idx + 3] = (total * 255.0).round().min(255.0) as u8;
        }
    }
    (buf, S as u32)
}

/// Actualiza el icono dinámico (anillo de sesión) y el tooltip de la bandeja.
/// `remaining` = fracción restante de la sesión (None si no hay datos); `severity`
/// fija el color. Llamado por el sondeo de `/usage`.
pub fn set_gauge(app: &AppHandle, remaining: Option<f64>, severity: &str, tooltip: &str) {
    let Some(tray) = app.tray_by_id("main-tray") else {
        return;
    };
    // Color fijo: ignoramos la severidad para el color del icono (no el tooltip).
    let sev = if TRAY_STATIC.load(Ordering::Relaxed) { "normal" } else { severity };
    let (rgba, size) = gauge_icon(remaining, severity_rgb(sev));
    let _ = tray.set_icon(Some(Image::new_owned(rgba, size, size)));
    let _ = tray.set_tooltip(Some(tooltip));
}

/// Alterna mostrar/ocultar la ventana principal y sincroniza el texto del
/// menú de bandeja + un evento para el frontend.
pub fn toggle_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let visible = window.is_visible().unwrap_or(false);
    if visible {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
    let now_visible = !visible;

    if let Some(state) = app.try_state::<TrayState>() {
        let _ = state.toggle.set_text(if now_visible { HIDE_TEXT } else { SHOW_TEXT });
    }

    use tauri::Emitter;
    let _ = app.emit("window://visibility-changed", serde_json::json!({ "visible": now_visible }));
}

/// Salida limpia: persiste el almacén local y termina el proceso. El plugin
/// `store` ya hace flush en `cleanup_before_exit`, pero forzamos un guardado
/// explícito del store de configuración por si tuviera cambios pendientes. La usa
/// tanto el menú "Salir" como el cierre por señal `--quit` (hook SessionEnd).
///
/// Las preferencias de la UI viven en `localStorage` (clave `widget-prefs`), no
/// en este store. Algunos ajustes se guardan con DEBOUNCE (tamaño tras
/// redimensionar, ancla tras un reacomodo): si saliéramos de inmediato, el último
/// podría perderse. Por eso AVISAMOS al frontend (`app://will-quit`) para que
/// vacíe lo pendiente a `localStorage` y le damos un margen breve antes de
/// terminar, de modo que la webview alcance a persistir a disco.
pub(crate) fn quit_clean(app: &AppHandle) {
    use tauri::Emitter;
    use tauri_plugin_store::StoreExt;
    let _ = app.emit("app://will-quit", ());
    if let Ok(store) = app.store("config.json") {
        let _ = store.save();
    }
    // Salimos desde un hilo aparte tras un respiro: así el bucle de eventos sigue
    // vivo y entrega `app://will-quit` (la webview vacía y persiste localStorage)
    // antes del `exit`. El margen es corto para no demorar el cierre.
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        handle.exit(0);
    });
}
