// resize.js — Redimensionado por el tirador de la esquina (arrastre NATIVO del SO,
// proporción bloqueada) y la ÚNICA fuente de verdad del escalado: el handler
// `onResized`, que reescala el contenido con zoom nativo ante cualquier cambio de
// tamaño y coalesce las ráfagas a uno por frame.
//
// Dirección "East" (solo ancho): la proporción está BLOQUEADA, lo que es 1 grado
// de libertad. Si dejáramos al SO controlar los dos ejes (SouthEast), tendríamos
// que reescribir el alto cada frame, peleando contra el arrastre -> tirones.
// Controlando solo el ancho, el alto lo fijamos nosotros (= ancho / proporción).

import { win, currentMonitor, LogicalSize, PhysicalPosition } from "./tauri.js";
import { prefs, savePrefs } from "./prefs.js";
import { ui } from "./state.js";
import { clamp, BASE_W, BASE_H, MIN_SCALE, MAX_SCALE, fitMaxScale, applyZoom, getSF, setSF } from "./geometry.js";
import { setPos, readAnchor } from "./anchor.js";

// Al pulsar el agarre de la esquina, arranca el resize nativo y acota el tope al
// espacio que queda hasta el borde del monitor (sin pelear desde JS).
export function startGripResize(e) {
  if (e.button !== 0) return; // solo botón primario
  const expanded = !prefs.collapsed || ui.peeking;
  if (!expanded) return; // en píldora no se redimensiona
  e.preventDefault();
  // Arranca el arrastre de inmediato (mientras el botón sigue pulsado).
  win.startResizeDragging("East").catch((err) => console.error("startResizeDragging:", err));
  prepResizeBounds();
}

// Ajusta el tamaño MÁXIMO al espacio disponible desde la esquina sup-izq de la
// ventana hasta el borde inf-der del monitor (el arrastre se crece hacia allí).
async function prepResizeBounds() {
  try {
    const mon = await currentMonitor();
    if (!mon) return;
    const sf = mon.scaleFactor || (await win.scaleFactor()) || 1;
    setSF(sf);
    const pos = await win.outerPosition(); // físico
    const fitW = (mon.position.x + mon.size.width - pos.x) / sf;
    const fitH = (mon.position.y + mon.size.height - pos.y) / sf;
    const fitScale = clamp(Math.min(fitW / BASE_W, fitH / BASE_H, MAX_SCALE), MIN_SCALE, MAX_SCALE);
    await win.setMaxSize(new LogicalSize(Math.round(BASE_W * fitScale), Math.round(BASE_H * fitScale)));
  } catch (e) {
    console.error("prepResizeBounds:", e);
  }
}

// Tras asentarse el redimensionado: persiste el tamaño, restaura el tope global y
// (por si acaso) reubica la ventana dentro del monitor.
async function settleResize(w, h) {
  prefs.fullSize = { w, h };
  try {
    const mon = await currentMonitor();
    // Restaura el tope al máximo que CABE en este monitor (no un valor estático),
    // para que el SO siga impidiendo rebasar la pantalla en el siguiente arrastre.
    const fit = fitMaxScale(mon);
    await win.setMaxSize(new LogicalSize(Math.round(BASE_W * fit), Math.round(BASE_H * fit)));
    if (mon) {
      const pos = await win.outerPosition();
      const size = await win.outerSize();
      const maxX = mon.position.x + mon.size.width - size.width;
      const maxY = mon.position.y + mon.size.height - size.height;
      const nx = maxX >= mon.position.x ? clamp(pos.x, mon.position.x, maxX) : mon.position.x;
      const ny = maxY >= mon.position.y ? clamp(pos.y, mon.position.y, maxY) : mon.position.y;
      if (nx !== pos.x || ny !== pos.y) {
        await setPos(new PhysicalPosition(Math.round(nx), Math.round(ny)));
      }
      if (prefs.rememberPosition) {
        prefs.position = await readAnchor();
      }
    }
  } catch (e) {
    console.error("settleResize:", e);
  }
  savePrefs();
}

// ÚNICA fuente de verdad del escalado. Ante CUALQUIER cambio de tamaño (arrastre
// nativo del tirador, restaurar al arrancar, cambio de monitor/DPI…). Para que sea
// FLUIDO, los eventos de resize (que llegan a ráfagas) se coalescen a uno por frame
// con requestAnimationFrame y el trabajo es "fire-and-forget" (sin await caliente).
let settleTimer = null;
let pendingPayload = null;
let resizeRaf = 0;

win.onResized(({ payload }) => {
  // Durante el morph píldora<->completo, el tamaño lo conduce el bucle de Rust y el
  // zoom lo fija runMorph: no intervenimos (evita pelear con el bloqueo de
  // proporción y los reescalados encolados).
  if (ui.animating) return;
  if (prefs.collapsed && !ui.peeking && !ui.moving) {
    // Píldora: tamaño fijo, sin zoom. Además CANCELA cualquier reescalado de modo
    // completo que quedara en vuelo (rAF o settle): si no, ese trabajo encolado se
    // ejecutaría tras el colapso e inflaría la ventana ya con forma de píldora el
    // bug de la "píldora gigante" intermitente.
    pendingPayload = null;
    if (resizeRaf) {
      cancelAnimationFrame(resizeRaf);
      resizeRaf = 0;
    }
    clearTimeout(settleTimer);
    applyZoom(1);
    return;
  }
  pendingPayload = payload;
  if (!resizeRaf) resizeRaf = requestAnimationFrame(processResize);
});

function processResize() {
  resizeRaf = 0;
  const payload = pendingPayload;
  pendingPayload = null;
  if (!payload) return;
  // Si entretanto colapsamos a píldora, NO reescalar: inflar ahora dejaría la
  // ventana grande con la forma de píldora (borde 999px) ya aplicada.
  if (prefs.collapsed && !ui.peeking && !ui.moving) {
    applyZoom(1);
    return;
  }

  const sf = getSF() || 1;
  const logicalW = payload.width / sf;
  const logicalH = payload.height / sf;
  // Escala por el ANCHO (el único eje que arrastra el SO en modo "East"). El alto
  // lo derivamos de la proporción, sin pelear con el arrastre.
  const scale = clamp(logicalW / BASE_W, MIN_SCALE, MAX_SCALE);
  applyZoom(scale);

  // Fija el alto a la proporción BASE. Converge solo: cuando ya coincide no se
  // vuelve a llamar a setSize. Fire-and-forget para no bloquear el frame.
  const wantW = Math.round(BASE_W * scale);
  const wantH = Math.round(BASE_H * scale);
  if (Math.abs(logicalH - wantH) > 1 || Math.abs(logicalW - wantW) > 1) {
    win.setSize(new LogicalSize(wantW, wantH)).catch((e) => console.error("resize lock:", e));
  }

  clearTimeout(settleTimer);
  settleTimer = setTimeout(() => settleResize(wantW, wantH), 160);
}
