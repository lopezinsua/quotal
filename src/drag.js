// drag.js — Mover la ventana (arrastre) + persistir posición.
//
// El gesto de mover lo controlamos NOSOTROS con eventos de puntero (en lugar de
// `data-tauri-drag-region`), para poder MOSTRAR LA TARJETA COMPLETA durante todo el
// movimiento (como pineada) aunque se arranque desde la píldora, y volver al reposo
// al soltar. Un gesto que no supera el umbral es un CLICK (la píldora lo usa para
// fijarse), no un arrastre. El fin del arrastre se detecta por el soltado del botón,
// con una red de seguridad (`pointermove` sin botones) por si el arrastre nativo del
// SO se traga el `pointerup`.

import { win, currentMonitor, LogicalSize } from "./tauri.js";
import { el } from "./dom.js";
import { prefs, onFlushPrefs } from "./prefs.js";
import { ui } from "./state.js";
import { fullSizeFor, fitMaxScale, applyZoom, FULL_MIN, BASE_W, BASE_H } from "./geometry.js";
import { captureAnchor, isSuppressed } from "./anchor.js";

// Persistir la posición: en un arrastre REAL (`ui.moving`) o estando desplegada de
// forma estable. Ignora reposicionamientos propios (suppress) y los reanclados al
// asomar (peeking sin mover).
let moveIdleTimer = null;
let savePosTimer = null;
let savePosPending = false; // hay un guardado de ancla con debounce sin vaciar?
win.onMoved(() => {
  // Respaldo de fin de arrastre: si el SO se tragó el `pointerup`/`mouseup`, el
  // movimiento se da por terminado tras un rato sin desplazarse. NO se dispara
  // durante un arrastre normal (los eventos `onMoved` llegan sin parar) ni con el
  // botón pulsado, así que no provoca el colapso a media maniobra.
  if (ui.moving) {
    clearTimeout(moveIdleTimer);
    moveIdleTimer = setTimeout(endMove, 300);
    return; // durante el arrastre, el guardado del ancla lo hace endMove (al soltar)
  }
  if (isSuppressed()) return;
  if (ui.peeking) return;
  if (!prefs.rememberPosition) return;
  // Movimiento no iniciado por nosotros (p. ej. reacomodo del SO) estando en
  // reposo: persistimos el ANCLA (esquina + coordenada), debounced para no leer
  // tamaño/monitor en cada evento.
  clearTimeout(savePosTimer);
  savePosPending = true;
  savePosTimer = setTimeout(() => {
    savePosPending = false;
    captureAnchor().catch((e) => console.error("saveAnchor:", e));
  }, 140);
});

// Si la ventana se oculta/cierra con un guardado de ancla aún pendiente (el
// debounce de 140 ms no ha saltado), vaciamos ya — así no se pierde la última
// posición. Es "best-effort": `captureAnchor` es asíncrono (lee posición por
// IPC), por lo que al ocultar a bandeja completa; en un cierre duro puede no
// llegar, pero el arrastre normal del usuario ya persiste al soltar (endMove).
onFlushPrefs(() => {
  if (!savePosPending) return;
  savePosPending = false;
  clearTimeout(savePosTimer);
  captureAnchor().catch((e) => console.error("saveAnchor:", e));
});

const MOVE_THRESHOLD = 3; // px de holgura para distinguir click de arrastre

// Termina el gesto: quita los listeners y cierra el movimiento (o dispara el click
// si no llegó a arrastrarse). Compartido por ambos modos.
function finishGesture(onMove, onUp, dragging, onClick) {
  if (onMove) window.removeEventListener("pointermove", onMove);
  window.removeEventListener("pointerup", onUp);
  window.removeEventListener("pointercancel", onUp);
  window.removeEventListener("mouseup", onUp);
  if (dragging) endMove();
  else if (typeof onClick === "function") onClick();
}

// Engancha el gesto de mover a un elemento. `onClick` se invoca si fue un click sin
// arrastre. `immediate` arranca el arrastre YA al pulsar (sin umbral): así no hay
// desfase entre cursor y ventana — ideal para la cabecera, que no tiene acción de
// click. La píldora usa umbral para distinguir click (fijar) de arrastre.
export function attachMoveHandle(handle, onClick, immediate = false) {
  handle.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return; // solo botón primario
    // No arrastrar desde controles interactivos ni desde el tirador de resize.
    if (e.target.closest("button, select, input, a, #resize-grip")) return;

    if (immediate) {
      // Arranque inmediato: el SO agarra la ventana en el punto exacto del cursor,
      // sin rezago. El soltado cierra el movimiento.
      beginMove();
      const onUp = () => finishGesture(null, onUp, true);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("pointercancel", onUp);
      window.addEventListener("mouseup", onUp);
      return;
    }

    const sx = e.clientX;
    const sy = e.clientY;
    let dragging = false;
    const onMove = (ev) => {
      if (dragging) return;
      if (Math.abs(ev.clientX - sx) < MOVE_THRESHOLD && Math.abs(ev.clientY - sy) < MOVE_THRESHOLD) return;
      dragging = true;
      beginMove();
    };
    const onUp = () => finishGesture(onMove, onUp, dragging, onClick);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    window.addEventListener("mouseup", onUp); // backstop si el SO se traga pointerup
  });
}

// Arranca el movimiento: marca el estado, asegura la vista completa (expandiendo EN
// EL SITIO si estaba en píldora, sin reanclar para no dar un salto) y delega el
// arrastre al gestor de ventanas del SO.
async function beginMove() {
  ui.moving = true;
  if (el.full.classList.contains("hidden")) {
    await expandInPlace();
  }
  try {
    await win.startDragging();
  } catch (err) {
    console.error("startDragging:", err);
    endMove();
  }
}

// Muestra la tarjeta completa SIN reposicionar la ventana (crece desde su esquina
// actual): versión "ligera" de applyLayout para el arranque del arrastre.
async function expandInPlace() {
  el.full.classList.remove("hidden");
  el.pill.classList.add("hidden");
  el.card.classList.remove("is-pill");
  try {
    const mon = await currentMonitor().catch(() => null);
    const full = fullSizeFor(mon);
    const fit = fitMaxScale(mon);
    applyZoom(full.scale);
    await win.setMinSize(new LogicalSize(FULL_MIN.w, FULL_MIN.h));
    await win.setMaxSize(new LogicalSize(Math.round(BASE_W * fit), Math.round(BASE_H * fit)));
    await win.setSize(new LogicalSize(full.w, full.h));
    // La vista ya está en completo: registra el estado para que, al soltar el
    // arrastre, no se dispare un morph espurio si se queda desplegada.
    ui.prevExpanded = true;
  } catch (e) {
    console.error("expandInPlace:", e);
  }
}

// Fin del movimiento: limpia el estado y avisa para que controls.js decida el
// reposo (seguir asomada si el ratón sigue encima, o volver a la píldora).
async function endMove() {
  if (!ui.moving) return;
  clearTimeout(moveIdleTimer);
  ui.moving = false;
  // Captura el ancla en el punto donde se soltó ANTES de avisar del fin: así el
  // colapso a píldora (que reposiciona desde el ancla) usa ya la posición nueva y
  // no la previa al arrastre (si no, el widget "volvería" a su sitio anterior).
  if (prefs.rememberPosition) await captureAnchor();
  window.dispatchEvent(new Event("widget:move-end"));
}
