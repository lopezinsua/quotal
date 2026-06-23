// geometry.js — Matemática de tamaño/escala de la ventana y utilidades de
// monitor. Es el módulo HOJA de la geometría: no depende de anchor/drag/resize,
// solo de Tauri y las preferencias. Aquí viven las constantes de escala, el
// cálculo del tope que cabe en pantalla, el zoom NATIVO del contenido, el factor
// de escala cacheado y el selector de monitor.

import { win, webview, currentMonitor, primaryMonitor } from "./tauri.js";
import { fullSize, SIZE_FULL_DEFAULT } from "./prefs.js";

// Escala del contenido. El lienzo (#full) mide BASE fijo y se reescala con el
// ZOOM NATIVO del webview. Bloqueamos la proporción BASE para que al escalar no
// queden huecos ni recortes; el factor va de MIN a MAX.
export const BASE_W = SIZE_FULL_DEFAULT.w;
export const BASE_H = SIZE_FULL_DEFAULT.h;
export const MIN_SCALE = 0.7;
export const MAX_SCALE = 3;
export const clamp = (v, a, b) => Math.max(a, Math.min(v, b));

// Límites de tamaño en modo completo (proporcionales a BASE -> tope y mínimo caen
// sobre la línea de proporción). El SO los impone durante el arrastre nativo, así
// no hay que "pelear" desde JS: el resize se detiene solo en el mínimo/máximo.
export const FULL_MIN = { w: Math.round(BASE_W * MIN_SCALE), h: Math.round(BASE_H * MIN_SCALE) };

// Margen de seguridad al borde de pantalla (lógicos) para el tope dinámico.
const SCREEN_MARGIN = 12;

// Factor de escala del monitor, cacheado (se refresca al iniciar y en cada
// arrastre). Evita un `await scaleFactor()` en el camino caliente del resize.
// Vive aquí (no como `export let`) para poder mutarlo desde varios módulos a
// través de los accesores: las exportaciones `let` son de solo lectura.
let cachedSF = 1;
win.scaleFactor().then((sf) => { if (sf) cachedSF = sf; }).catch(() => {});

/// Factor de escala cacheado del monitor (lectura).
export function getSF() {
  return cachedSF;
}
/// Actualiza el factor de escala cacheado (lo refresca el resize al medir monitor).
export function setSF(v) {
  if (v) cachedSF = v;
}

// Escala máxima que cabe ENTERA en un monitor (descontando el margen), nunca por
// encima de MAX_SCALE. Es el tope REAL: garantiza que la ventana jamás rebase la
// pantalla, aunque el monitor sea pequeño o el tamaño guardado fuese mayor. La
// proporción está bloqueada, así que basta el eje más restrictivo (ancho o alto).
export function fitMaxScale(mon) {
  if (!mon) return MAX_SCALE;
  const sf = mon.scaleFactor || getSF() || 1;
  const availW = mon.size.width / sf - SCREEN_MARGIN * 2;
  const availH = mon.size.height / sf - SCREEN_MARGIN * 2;
  return clamp(Math.min(availW / BASE_W, availH / BASE_H, MAX_SCALE), MIN_SCALE, MAX_SCALE);
}

// Tamaño completo efectivo, acotado a lo que cabe en el monitor dado.
export function fullSizeFor(mon) {
  const fit = fitMaxScale(mon);
  const s = clamp(fullSize().w / BASE_W, MIN_SCALE, fit);
  return { w: Math.round(BASE_W * s), h: Math.round(BASE_H * s), scale: s, fit };
}

// Aplica la escala al contenido con el ZOOM NATIVO del webview. Es la forma
// CORRECTA y fiable de reescalar en Tauri: re-renderiza toda la página (texto
// incluido) nítida, igual que el zoom del navegador, y funciona en Win/mac/Linux
// — a diferencia del CSS `zoom`, que WebView2 ignora de forma intermitente y por
// eso "las letras no eran acordes al tamaño de la ventana".
export function applyZoom(s) {
  const z = clamp(s, MIN_SCALE, MAX_SCALE);
  if (webview && webview.setZoom) {
    webview.setZoom(z).catch((e) => console.error("setZoom:", e));
  }
}

// Monitor donde está la ventana; si falla, el primario.
export async function pickMonitor() {
  try {
    const m = await currentMonitor();
    if (m) return m;
  } catch (e) {
    /* ignore */
  }
  try {
    const m = await primaryMonitor();
    if (m) return m;
  } catch (e) {
    /* ignore */
  }
  return null;
}
