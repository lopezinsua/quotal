// window.js — Orquestador de la geometría de la ventana: conmuta entre modo
// completo y píldora (con morph fluido) y mantiene clase + tamaño + posición
// coherentes. La mecánica está repartida en módulos cohesivos:
//
//   geometry.js-> escala/tamaño, zoom nativo, monitor, factor de escala.
//   anchor.js  -> anclaje por esquina, IPC de posición/tamaño, restaurar/colocar.
//   resize.js  -> tirador de redimensionado + onResized (única fuente del escalado).
//   drag.js    -> gesto de mover la ventana + onMoved.
//
// Este módulo re exporta la API pública que consumen controls.js y main.js, de modo
// que importar desde "./window.js" sigue funcionando igual tras la división.

import { invoke, win, currentMonitor, LogicalSize } from "./tauri.js";
import { el } from "./dom.js";
import { prefs, pillSize } from "./prefs.js";
import { ui } from "./state.js";
import { fitMaxScale, fullSizeFor, applyZoom, BASE_W, BASE_H, FULL_MIN, getSF } from "./geometry.js";
import { normAnchor, readAnchor, anchorToTopLeft, setBounds } from "./anchor.js";

// API pública re-exportada (la usan controls.js / main.js sin cambios).
export { setPos, gridTarget, anchorPosition, captureAnchor, restorePosition } from "./anchor.js";
export { startGripResize } from "./resize.js";
export { attachMoveHandle } from "./drag.js";

// Serialización de applyLayout: una sola pasada a la vez, y si llegan más peticiones
// mientras corre, se re-ejecuta UNA vez más al terminar leyendo el estado más
// reciente. Así nunca quedan la CLASE (píldora) y el TAMAÑO de la ventana
// desincronizados (el bug del "círculo grande": clase de píldora puesta pero la
// ventana sin encoger).
let applying = false;
let applyAgain = false;

// Respeta la preferencia del SO de reducir movimiento (sin morph -> cambio seco).
const REDUCED_MOTION =
  typeof window !== "undefined" &&
  window.matchMedia &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// Conmuta el modo (completo / píldora) y fija el tamaño de la ventana. Punto de
// entrada SERIALIZADO: garantiza que solo corre una pasada a la vez y que, si llegan
// peticiones mientras tanto (p. ej. hover/leave + clic), se re-ejecuta al terminar
// con el estado más reciente — de modo que la clase de píldora y el tamaño de la
// ventana SIEMPRE acaban coherentes.
export async function applyLayout() {
  applyAgain = true;
  if (applying) return;
  applying = true;
  try {
    while (applyAgain) {
      applyAgain = false;
      await applyLayoutOnce();
    }
  } finally {
    applying = false;
  }
}

// Una pasada: lee el estado AHORA (no antes de encolar) y deja clase + tamaño +
// posición coherentes. En completo usa el tamaño que el usuario dejó (o el default);
// en píldora, el tamaño compacto del estilo activo.
async function applyLayoutOnce() {
  // Completa si: está pineada, asomada por hover, o se está arrastrando (durante el
  // movimiento mantenemos la vista completa, como pineada).
  const expanded = !prefs.collapsed || ui.peeking || ui.moving;
  // Animamos el cambio de modo (píldora↔completo) salvo durante el arrastre o si el
  // usuario pidió menos movimiento.
  const animate =
    ui.prevExpanded !== null && ui.prevExpanded !== expanded && !ui.moving && !REDUCED_MOTION;
  // Orden anti-parpadeo (camino instantáneo): al EXPANDIR mostramos la tarjeta
  // completa ANTES de crecer. (En el morph animado el contenido lo gestiona runMorph
  // con crossfade, así que no tocamos clases aquí si vamos a animar.)
  if (expanded && !animate) {
    el.full.classList.remove("hidden");
    el.pill.classList.add("hidden");
    el.card.classList.remove("is-pill");
  }

  try {
    // Tope REAL acotado al monitor actual: ni el tamaño guardado ni el máximo pueden
    // hacer que la ventana rebase la pantalla.
    const mon = await currentMonitor().catch(() => null);
    const fit = fitMaxScale(mon);
    const fullDims = fullSizeFor(mon);
    const pill = pillSize();
    const target = expanded ? { w: fullDims.w, h: fullDims.h } : pill;
    // Límites de tamaño que el SO impondrá: en completo, [mín, tope-que-cabe]; en
    // píldora, fijo (no redimensionable). Para poder ENCOGER por debajo del mínimo
    // anterior, bajamos primero el mínimo y solo después fijamos el tamaño.
    const lo = expanded ? FULL_MIN : pill;
    const hi = expanded ? { w: Math.round(BASE_W * fit), h: Math.round(BASE_H * fit) } : pill;
    await win.setMinSize(new LogicalSize(lo.w, lo.h));
    await win.setMaxSize(new LogicalSize(hi.w, hi.h));
    const sf = (mon && mon.scaleFactor) || getSF() || 1;
    const physW = Math.round(target.w * sf);
    const physH = Math.round(target.h * sf);
    const anchor = normAnchor(prefs.position) || (await readAnchor());
    const tl = anchor ? await anchorToTopLeft(anchor, physW, physH, mon) : null;

    if (animate && tl) {
      // Morph fluido: la ventana crece/encoge suavemente (bucle en Rust) mientras el
      // contenido hace crossfade y el borde se redondea.
      await runMorph(expanded, fullDims.scale, tl, physW, physH);
      ui.prevExpanded = expanded;
      return;
    }

    // Camino instantáneo (pasada sin cambio de modo o durante arrastre): tamaño +
    // posición JUNTOS de forma atómica, manteniendo fija la esquina anclada.
    applyZoom(expanded ? fullDims.scale : 1);
    if (tl) {
      await setBounds(tl.x, tl.y, physW, physH);
    } else {
      await win.setSize(new LogicalSize(target.w, target.h));
    }
  } catch (e) {
    console.error("applyLayoutOnce:", e);
    ui.animating = false; // por si un morph quedó a medias, no bloquear onResized
  }

  // Ya con el tamaño aplicado: si vamos a píldora, ahora sí le damos su forma
  // (después de haber encogido), evitando el frame de "círculo gigante".
  if (!expanded) {
    el.full.classList.add("hidden");
    el.pill.classList.remove("hidden");
    el.card.classList.add("is-pill");
  }
  ui.prevExpanded = expanded;
}

// Duración del morph píldora<->completo (ms). Igual en JS (crossfade/borde) y en el
// bucle de tamaño de la ventana (Rust), para que vayan sincronizados.
const MORPH_MS = 240;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Morph fluido entre píldora y completo: la VENTANA crece/encoge suavemente (bucle
// en Rust con easing) mientras el CONTENIDO hace crossfade y el borde se redondea
// (transición CSS). El zoom del completo se fija durante toda la transición (el
// contenido se revela/oculta con la ventana; el crossfade tapa el desajuste de
// proporción píldora↔tarjeta).
async function runMorph(expanded, fullScale, tl, physW, physH) {
  ui.animating = true;
  // Ambas vistas visibles para el crossfade (sin display:none).
  el.full.classList.remove("hidden");
  el.pill.classList.remove("hidden");
  el.card.classList.add("morphing");
  // Forma destino: la transición CSS de border-radius la anima (14px<->999px) a la vez
  // que la ventana cambia de tamaño-> se redondea en pastilla al encoger.
  el.card.classList.toggle("is-pill", !expanded);
  // Zoom del completo fijo durante el morph (sin pelear con onResized, que se inhibe
  // con ui.animating).
  applyZoom(fullScale);
  // Crossfade + ESCALA: además de la opacidad, el contenido crece/encoge un pelín
  // mientras aparece/desaparece, así el morph "fluye" hacia su forma en vez de un
  // simple fundido. Fijamos estados INICIALES, forzamos reflow y ponemos los
  // FINALES para disparar la transición CSS (opacidad + transform).
  el.full.style.opacity = expanded ? "0" : "1";
  el.pill.style.opacity = expanded ? "1" : "0";
  el.full.style.transform = expanded ? "scale(0.96)" : "scale(1)";
  el.pill.style.transform = expanded ? "scale(1)" : "scale(0.92)";
  void el.card.offsetWidth;
  el.full.style.opacity = expanded ? "1" : "0";
  el.pill.style.opacity = expanded ? "0" : "1";
  el.full.style.transform = expanded ? "scale(1)" : "scale(0.96)";
  el.pill.style.transform = expanded ? "scale(0.92)" : "scale(1)";

  invoke("animate_bounds", { x: tl.x, y: tl.y, w: physW, h: physH, ms: MORPH_MS }).catch((e) =>
    console.error("animate_bounds:", e),
  );

  await sleep(MORPH_MS + 40);

  // Estado final canónico (por si otra animación lo superó, comprobamos que el modo
  // objetivo sigue siendo el vigente).
  ui.animating = false;
  el.card.classList.remove("morphing");
  el.full.style.opacity = "";
  el.pill.style.opacity = "";
  el.full.style.transform = "";
  el.pill.style.transform = "";
  if (expanded) {
    el.full.classList.remove("hidden");
    el.pill.classList.add("hidden");
    el.card.classList.remove("is-pill");
    applyZoom(fullScale);
  } else {
    el.full.classList.add("hidden");
    el.pill.classList.remove("hidden");
    el.card.classList.add("is-pill");
    applyZoom(1);
  }
  // Corrige tamaño/posición exactos (el bucle pudo dejar +-1px de rounding).
  await setBounds(tl.x, tl.y, physW, physH);
}
