// anchor.js - Anclaje de la ventana por la ESQUINA de pantalla más cercana, IPC de
// posición/tamaño (set_bounds / setPosition) y restauración/colocación.
//
// La posición se guarda como la ESQUINA del widget más próxima a un borde de
// pantalla ({right, bottom}) y la coordenada física de esa esquina ({x, y}). Ambos
// modos (píldora / completo) mantienen FIJA esa esquina: la píldora se queda
// clavada donde está y el completo se DESPLIEGA HACIA EL INTERIOR de la pantalla
// (nunca contra el borde), así la píldora no se mueve. Al colapsar, la píldora
// vuelve exactamente a su esquina. Formatos antiguos → se descartan y se recae en
// la posición por defecto.

import { invoke, win, currentMonitor, availableMonitors, PhysicalPosition } from "./tauri.js";
import { prefs, savePrefs, fullSize, pillSize } from "./prefs.js";
import { ui } from "./state.js";
import { clamp, pickMonitor, getSF } from "./geometry.js";

// Silencia el guardado de posición durante un breve margen tras un movimiento
// PROPIO (set_bounds / setPosition), para que no se confunda con un arrastre del
// usuario. Lo lee el handler `onMoved` de drag.js a través de `isSuppressed()`.
let suppressMoveSave = false;
let suppressTimer = null;

/// Estamos silenciando el guardado de posición ahora mismo? (lo usa drag.js)
export function isSuppressed() {
  return suppressMoveSave;
}

// Arranca/renueva la ventana de supresión del guardado de posición.
function suppressSaves() {
  suppressMoveSave = true;
  if (suppressTimer) clearTimeout(suppressTimer);
  suppressTimer = setTimeout(() => {
    suppressMoveSave = false;
  }, 250);
}

// Descarta formatos de ancla antiguos: exige {x, y} finitos y {right, bottom} bool.
export function normAnchor(p) {
  if (!p || !Number.isFinite(p.x) || !Number.isFinite(p.y)) return null;
  if (typeof p.right !== "boolean" || typeof p.bottom !== "boolean") return null;
  return p;
}

// Lee el ancla de esquina de la ventana AHORA: la esquina más cercana a un borde
// de pantalla y su coordenada física. Fuente de verdad al mover/redimensionar/snap.
export async function readAnchor() {
  try {
    const mon = await currentMonitor().catch(() => null);
    const pos = await win.outerPosition(); // físico (top-left)
    const size = await win.outerSize(); // físico
    const mx = mon ? mon.position.x : pos.x;
    const my = mon ? mon.position.y : pos.y;
    const mw = mon ? mon.size.width : size.width;
    const mh = mon ? mon.size.height : size.height;
    // Esquina más próxima a un borde (hueco menor). La ventana crecerá hacia el
    // lado contrario (el interior), así que el borde anclado nunca topa pantalla.
    const right = mx + mw - (pos.x + size.width) < pos.x - mx;
    const bottom = my + mh - (pos.y + size.height) < pos.y - my;
    return {
      right,
      bottom,
      x: right ? pos.x + size.width : pos.x,
      y: bottom ? pos.y + size.height : pos.y,
    };
  } catch (e) {
    console.error("readAnchor:", e);
    return null;
  }
}

// Deriva el top-left FÍSICO para un tamaño FÍSICO manteniendo FIJA la esquina
// anclada (el widget crece/encoge hacia el interior). Acota al monitor por si
// acaso (normalmente no hace falta: ya crece hacia dentro).
export async function anchorToTopLeft(anchor, physW, physH, mon) {
  const m = mon || (await pickMonitor());
  let x = anchor.right ? anchor.x - physW : anchor.x;
  let y = anchor.bottom ? anchor.y - physH : anchor.y;
  if (m) {
    const maxX = m.position.x + m.size.width - physW;
    const maxY = m.position.y + m.size.height - physH;
    x = maxX >= m.position.x ? clamp(x, m.position.x, maxX) : m.position.x;
    y = maxY >= m.position.y ? clamp(y, m.position.y, maxY) : m.position.y;
  }
  return { x: Math.round(x), y: Math.round(y) };
}

// Aplica posición + tamaño JUNTOS en el backend (un IPC, sin await intermedio del
// lado JS -> sin repintado intermedio). Silencia el guardado como en `setPos`.
export async function setBounds(x, y, w, h) {
  suppressSaves();
  try {
    await invoke("set_bounds", { x, y, w, h });
  } catch (e) {
    console.error("set_bounds:", e);
  }
}

// Evita que nuestros propios `setPosition` se guarden como si fueran un arrastre
// del usuario: silenciamos el guardado durante un breve margen tras moverla.
export async function setPos(p) {
  suppressSaves();
  await win.setPosition(p);
}

// Captura el ancla de esquina actual y la persiste. La usan "recordar posición",
// el selector de posición (snap) y el fin de arrastre/redimensionado.
export async function captureAnchor() {
  const a = await readAnchor();
  if (a) {
    prefs.position = a;
    savePrefs();
  }
}

// Posición por defecto: esquina superior derecha del monitor, con margen.
async function defaultTopRight(target) {
  const mon = await pickMonitor();
  if (!mon) return null;
  const scale = mon.scaleFactor || 1;
  const margin = 12 * scale;
  const tw = target.w * scale;
  const x = mon.position.x + mon.size.width - tw - margin;
  const y = mon.position.y + margin;
  return new PhysicalPosition(Math.round(x), Math.round(y));
}

// La posición (físicos) deja la ventana visible en algún monitor?
async function positionIsOnScreen(x, y, target) {
  let mons = [];
  try {
    mons = await availableMonitors();
  } catch (e) {
    /* ignore */
  }
  if (!mons || !mons.length) {
    const m = await pickMonitor();
    if (m) mons = [m];
  }
  if (!mons.length) return false;
  const scale = mons[0].scaleFactor || 1;
  const tw = target.w * scale;
  const th = target.h * scale;
  const edge = 8; // exige al menos esta porción visible
  return mons.some((m) => {
    const mx = m.position.x;
    const my = m.position.y;
    const mw = m.size.width;
    const mh = m.size.height;
    return x + tw > mx + edge && x < mx + mw - edge && y + th > my + edge && y < my + mh - edge;
  });
}

// Restaura la posición guardada; si no hay o quedó fuera de pantalla, usa el
// sitio por defecto (arriba a la derecha).
export async function restorePosition() {
  const target = !prefs.collapsed ? fullSize() : pillSize();
  try {
    const anchor = normAnchor(prefs.position);
    if (prefs.rememberPosition && anchor) {
      // Restauramos sobre la MISMA esquina anclada (la píldora vuelve a su sitio).
      const mon = await pickMonitor();
      const sf = (mon && mon.scaleFactor) || getSF() || 1;
      const physW = Math.round(target.w * sf);
      const physH = Math.round(target.h * sf);
      const tl = await anchorToTopLeft(anchor, physW, physH, mon);
      if (tl && (await positionIsOnScreen(tl.x, tl.y, target))) {
        await setPos(new PhysicalPosition(tl.x, tl.y));
        return;
      }
    }
  } catch (e) {
    console.error("restorePosition:", e);
  }
  const def = await defaultTopRight(target);
  if (def) {
    await setPos(def);
    // Poblamos el ancla (centro) para que los siguientes cambios de modo sean
    // consistentes desde el primer momento, aunque el usuario no haya movido aún.
    await captureAnchor();
  }
}

// Tamaño efectivo de la ventana ahora mismo (para anclar bien los bordes).
export function gridTarget() {
  return !prefs.collapsed || ui.peeking ? fullSize() : pillSize();
}

// Posición física para un anclaje {h: left|center|right, v: top|middle|bottom}.
export async function anchorPosition(h, v, target) {
  const mon = await pickMonitor();
  if (!mon) return null;
  const s = mon.scaleFactor || 1;
  const m = 12 * s;
  const tw = target.w * s;
  const th = target.h * s;
  const mx = mon.position.x;
  const my = mon.position.y;
  const mw = mon.size.width;
  const mh = mon.size.height;
  const x = h === "left" ? mx + m : h === "right" ? mx + mw - tw - m : mx + (mw - tw) / 2;
  const y = v === "top" ? my + m : v === "bottom" ? my + mh - th - m : my + (mh - th) / 2;
  return new PhysicalPosition(Math.round(x), Math.round(y));
}
