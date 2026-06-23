// tauri.js — Punto único de acceso a la API de Tauri (inyectada en `window`).
// Centralizarla aquí evita repetir el destructuring y facilita testear/mockear.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow, currentMonitor, primaryMonitor, availableMonitors } =
  window.__TAURI__.window;
const { getCurrentWebview } = window.__TAURI__.webview;
const { LogicalSize, PhysicalPosition } = window.__TAURI__.dpi;

export { invoke, listen, currentMonitor, primaryMonitor, availableMonitors, LogicalSize, PhysicalPosition };

/// La ventana actual (singleton).
export const win = getCurrentWindow();

/// El webview actual (singleton). Necesario para el zoom NATIVO (`setZoom`), que
/// reescala el contenido de forma nítida y fiable en todas las plataformas a
/// diferencia del CSS `zoom`, que WebView2 ignora de forma intermitente.
export const webview = getCurrentWebview();
