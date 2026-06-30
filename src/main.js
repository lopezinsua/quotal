// main.js — Punto de entrada del frontend (sin framework).
//
// Solo orquesta: importa los módulos (cuyos efectos enganchan los listeners),
// suscribe el flujo de datos y arranca. La lógica vive en módulos cohesivos:
//   tauri.js    -> API de Tauri          prefs.js   -> preferencias + tamaños
//   dom.js      -> caché de nodos         state.js   -> estado de UI compartido
//   boot.js     -> arranque de i18n       format.js  -> formateadores
//   render.js   -> pintado                window.js  -> geometría/posición/peek
//   controls.js -> eventos/settings/idioma/refresco
//
// No hace polling: se suscribe a `usage://metrics-updated` y repinta cuando el
// backend empuja datos. Solo manda comandos cuando el usuario actúa.

import { applyStaticI18n } from "./i18n.js";
import { i18nReady } from "./boot.js";
import { invoke, listen, win } from "./tauri.js";
import { ui } from "./state.js";
import { render } from "./render.js";
import { applyLayout, restorePosition } from "./window.js";
import { applyVisualPrefs } from "./controls.js";
import { flushPrefs } from "./prefs.js";
import { notifyFromPayload } from "./notify.js";
import "./update.js"; // avisos de actualización y de dependencias (efectos al importar)

// Al resolver la tabla del idioma activo, traduce las cadenas estáticas del HTML.
i18nReady.then(() => applyStaticI18n());

// ---- Persistir el último ajuste al ocultar/cerrar ----
// Los guardados con debounce (tamaño tras redimensionar, ancla tras reacomodo)
// podrían perderse si la app se cierra antes de que salte su temporizador —p. ej.
// el hook SessionEnd lanza `--quit` y el proceso sale de inmediato. Vaciamos los
// pendientes cuando la webview se oculta a bandeja o se va a cerrar. `pagehide`
// es más fiable que `beforeunload` en webviews; cubrimos ambos por si acaso.
window.addEventListener("pagehide", flushPrefs);
window.addEventListener("beforeunload", flushPrefs);
// Cierre del backend (menú "Salir" o hook SessionEnd `--quit`): un `app.exit`
// duro no dispara `pagehide`, así que el backend nos avisa y espera un margen
// para que vaciemos lo pendiente a localStorage antes de terminar.
listen("app://will-quit", () => flushPrefs());

// ---- Flujo de datos ----
// Cada actualización de plan repinta y, además, alimenta el vigilante de umbral
// (decide por su cuenta si debe lanzar una notificación de escritorio).
function onMetrics(payload) {
  render(payload);
  notifyFromPayload(payload);
}
listen("usage://metrics-updated", (e) => onMetrics(e.payload));
invoke("get_metrics").then(onMetrics).catch((err) => console.error("get_metrics:", err));

// Refresco periódico de los contadores de cuenta atrás (por si no llega evento
// nuevo). Se PAUSA cuando la ventana se oculta en la bandeja: un widget escondido
// no necesita repintar, y así no gastamos CPU/GPU ni batería en segundo plano. Al
// volver a mostrarse repinta de inmediato para que la cuenta atrás no se vea
// "congelada".
const TICK_MS = 30000;
let tickTimer = null;
const tick = () => ui.lastPayload && render(ui.lastPayload);
function startTicking() {
  if (tickTimer) return;
  tick(); // repinta ya al reanudar (evita el salto al volver de la bandeja)
  tickTimer = setInterval(tick, TICK_MS);
}
function stopTicking() {
  clearInterval(tickTimer);
  tickTimer = null;
}
if (!document.hidden) startTicking();

// Un solo handler de visibilidad: al ocultar, vacía los guardados diferidos y
// detiene el refresco; al mostrar, reanuda el refresco.
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    flushPrefs();
    stopTicking();
  } else {
    startTicking();
  }
});

// ---- Arranque ----
(async function init() {
  try {
    await applyVisualPrefs();
    await applyLayout();
    // Colocar la ventana en su sitio recordado (o arriba a la derecha) DESPUÉS de
    // fijar el tamaño, para que el cálculo del borde use las dimensiones reales.
    await restorePosition();
  } catch (e) {
    console.error("init:", e);
  } finally {
    // La ventana NACE oculta (`visible:false` en tauri.conf.json) para no
    // parpadear en una posición/tamaño provisionales —p. ej. fuera de pantalla en
    // monitores pequeños— ni mostrar contenido sin estilar. La revelamos SOLO aquí,
    // ya con tamaño y posición definitivos. En `finally` para que un fallo arriba
    // no la deje invisible (solo accesible desde la bandeja).
    try {
      await win.show();
    } catch (e) {
      console.error("show:", e);
    }
  }
})();
