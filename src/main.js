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

// Al resolver la tabla del idioma activo, traduce las cadenas estáticas del HTML.
i18nReady.then(() => applyStaticI18n());

// ---- Flujo de datos ----
listen("usage://metrics-updated", (e) => render(e.payload));
invoke("get_metrics").then(render).catch((err) => console.error("get_metrics:", err));
// Refresca los contadores de cuenta atrás aunque no llegue evento nuevo.
setInterval(() => ui.lastPayload && render(ui.lastPayload), 30000);

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
