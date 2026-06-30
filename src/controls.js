// controls.js — Wiring de la interacción: toggles de configuración, selector de
// idioma, rejilla de posición, pin/píldora, asomo por hover, navegación de
// vistas y el botón de refresco. Registrar este módulo (importarlo) engancha
// todos los listeners; expone `applyVisualPrefs` para el arranque.

import { invoke, win } from "./tauri.js";
import { el } from "./dom.js";
import { prefs, savePrefs, PILL_STYLES, pillStyle, DEFAULT_BORDER_COLORS } from "./prefs.js";
import { ui } from "./state.js";
import { applyLayout, setPos, gridTarget, anchorPosition, captureAnchor, startGripResize, attachMoveHandle } from "./window.js";
import { render } from "./render.js";
import {
  t,
  setLocale,
  detectLocale,
  applyStaticI18n,
  autoLabel,
  isRTL,
  SUPPORTED,
} from "./i18n.js";
import { i18nReady } from "./boot.js";
import { ensureNotifyPermission, rearmNotifications } from "./notify.js";

// Flags locales a la interacción.
let settingsOpen = false;
let refreshing = false;

// Aplica el estilo de píldora vigente: una sola clase `pill--<estilo>` en #pill
// decide qué indicador se ve (el CSS oculta los demás). Fuente única de verdad,
// la usan el arranque y el selector.
function applyPillStyle() {
  const style = pillStyle();
  el.pill.classList.remove(...PILL_STYLES.map((s) => `pill--${s}`));
  el.pill.classList.add(`pill--${style}`);
  if (el.optPillStyle) el.optPillStyle.value = style;
}

// Aplica los colores del borde: por cada estado, inyecta la variable CSS en
// :root si hay color personalizado, o la quita para que mande el default del CSS
// (el hairline translúcido normal y los anillos amber/rojo). Además sincroniza
// los selectores de color con el valor vigente (custom o el de muestra default).
function applyBorderColors() {
  const root = document.documentElement.style;
  const bc = prefs.borderColors || {};
  const map = {
    normal: ["--border-normal", el.optBorderNormal],
    warn: ["--border-warn", el.optBorderWarn],
    crit: ["--border-crit", el.optBorderCrit],
  };
  for (const [key, [cssVar, input]] of Object.entries(map)) {
    const val = bc[key];
    if (val) root.setProperty(cssVar, val);
    else root.removeProperty(cssVar);
    if (input) input.value = val || DEFAULT_BORDER_COLORS[key];
  }
}

// Opción "iluminar borde con el uso": la clase `no-glow` apaga el anillo de
// aviso/crítico del MARCO (el resto del color de estado se mantiene).
function applyBorderGlow() {
  el.card.classList.toggle("no-glow", prefs.borderGlow === false);
}

// ---- Preferencias visuales (aplica al DOM + sincroniza checkboxes) ----
export async function applyVisualPrefs() {
  el.card.classList.toggle("translucent", prefs.translucent);
  el.contextLine.classList.toggle("hidden", !prefs.showContext);
  el.optTranslucent.checked = prefs.translucent;
  el.optOnTop.checked = prefs.onTop;
  el.optCollapsed.checked = prefs.collapsed;
  el.optShowContext.checked = prefs.showContext;
  el.optRememberPos.checked = prefs.rememberPosition;
  applyPillStyle();
  applyBorderColors();
  applyBorderGlow();
  el.optBorderGlow.checked = prefs.borderGlow !== false;
  el.optTrayStatic.checked = !!prefs.trayStaticColor;
  if (el.optNotify) el.optNotify.checked = !!prefs.notifyEnabled;
  if (el.optNotifyThreshold) el.optNotifyThreshold.value = String(prefs.notifyThreshold ?? 90);
  // Sincroniza la bandeja (backend) con la preferencia de color fijo.
  invoke("set_tray_static", { enabled: !!prefs.trayStaticColor }).catch((e) =>
    console.error("set_tray_static:", e),
  );
  // El estado real del auto-arranque vive en settings.json (backend).
  invoke("autostart_status")
    .then((on) => {
      el.optAutostart.checked = !!on;
    })
    .catch((e) => console.error("autostart_status:", e));
  invoke("shutdown_status")
    .then((on) => {
      el.optClose.checked = !!on;
    })
    .catch((e) => console.error("shutdown_status:", e));
  invoke("statusline_status")
    .then((on) => {
      el.optStatusline.checked = !!on;
    })
    .catch((e) => console.error("statusline_status:", e));
  updatePinUi();
  try {
    await win.setAlwaysOnTop(prefs.onTop);
  } catch (e) {
    console.error("setAlwaysOnTop:", e);
  }
}

// ---- Asomar al pasar el ratón + reposo tras mover ----
// El gesto de MOVER lo gestiona window.js (attachMoveHandle): durante el arrastre
// muestra la tarjeta completa. Aquí solo: (1) asomamos al hover, (2) decidimos el
// estado de reposo cuando termina un arrastre, (3) fijamos al clicar la píldora.

// Asomar al entrar el ratón (solo en píldora y si no se está arrastrando).
document.body.addEventListener("pointerenter", () => {
  ui.pointerInside = true;
  if (prefs.collapsed && !ui.peeking && !ui.moving) {
    ui.peeking = true;
    applyLayout();
  }
});
// Al salir el ratón, vuelve a la píldora. Durante un arrastre NO colapsa (lo
// decide el fin del arrastre, abajo). Cierra los ajustes al minimizar.
document.body.addEventListener("pointerleave", () => {
  ui.pointerInside = false;
  if (prefs.collapsed && ui.peeking && !ui.moving) {
    ui.peeking = false;
    if (settingsOpen) showSettings(false);
    applyLayout();
  }
});
// Fin de un arrastre: si está en píldora, queda asomada mientras el ratón siga
// encima; si lo soltaste fuera, vuelve a la píldora. Si está pineada, no se toca
// (se queda donde la soltaste).
window.addEventListener("widget:move-end", () => {
  if (!prefs.collapsed) return;
  ui.peeking = ui.pointerInside;
  if (!ui.pointerInside && settingsOpen) showSettings(false);
  applyLayout();
});

// Mover desde la cabecera (vista completa) y desde la píldora. En la píldora, un
// click sin arrastre la fija (pin).
attachMoveHandle(el.headerBar, null, true); // cabecera: arrastre inmediato (sin rezago)
attachMoveHandle(el.pill, () => {
  prefs.collapsed = false;
  ui.peeking = false;
  el.optCollapsed.checked = false;
  savePrefs();
  updatePinUi();
  applyLayout();
});

// Refleja en el botón pin si la ventana está fijada (desplegada) o suelta.
function updatePinUi() {
  const pinned = !prefs.collapsed;
  el.pinBtn.classList.toggle("pinned", pinned);
  el.pinBtn.title = t(pinned ? "pin_release" : "pin_fix");
}
 // Pin = toggle: fija desplegada (collapsed=false) o la suelta a píldora.
el.pinBtn.addEventListener("click", () => {
  prefs.collapsed = !prefs.collapsed;
  ui.peeking = false;
  el.optCollapsed.checked = prefs.collapsed;
  savePrefs();
  updatePinUi();
  applyLayout();
});

// ---- Toggles de configuración ----
el.optTranslucent.addEventListener("change", () => {
  prefs.translucent = el.optTranslucent.checked;
  savePrefs();
  applyVisualPrefs();
});
el.optOnTop.addEventListener("change", () => {
  prefs.onTop = el.optOnTop.checked;
  savePrefs();
  applyVisualPrefs();
});
el.optCollapsed.addEventListener("change", () => {
  prefs.collapsed = el.optCollapsed.checked;
  ui.peeking = false;
  savePrefs();
  updatePinUi();
  applyLayout();
});
el.optShowContext.addEventListener("change", () => {
  prefs.showContext = el.optShowContext.checked;
  el.contextLine.classList.toggle("hidden", !prefs.showContext);
  savePrefs();
});

// Iluminar borde con el uso: on/off del anillo de aviso/crítico del marco.
el.optBorderGlow.addEventListener("change", () => {
  prefs.borderGlow = el.optBorderGlow.checked;
  savePrefs();
  applyBorderGlow();
});

// Color fijo en la bandeja: el icono se queda en el color de marca (no cambia por
// severidad). Lo decide el backend (lo dibuja Rust); se lo comunicamos por IPC.
el.optTrayStatic.addEventListener("change", () => {
  prefs.trayStaticColor = el.optTrayStatic.checked;
  savePrefs();
  invoke("set_tray_static", { enabled: prefs.trayStaticColor }).catch((e) =>
    console.error("set_tray_static:", e),
  );
});

// Notificaciones de umbral: al activarlas pedimos permiso al SO; si se deniega,
// revertimos el toggle (no tendría efecto). El umbral solo guarda y re-arma para
// que el nuevo valor se respete en la próxima evaluación.
if (el.optNotify) {
  el.optNotify.addEventListener("change", async () => {
    const on = el.optNotify.checked;
    if (on) {
      const granted = await ensureNotifyPermission();
      if (!granted) {
        el.optNotify.checked = false; // sin permiso, no sirve activarlo
        return;
      }
    }
    prefs.notifyEnabled = on;
    savePrefs();
  });
}
if (el.optNotifyThreshold) {
  el.optNotifyThreshold.addEventListener("change", () => {
    prefs.notifyThreshold = Number(el.optNotifyThreshold.value) || 90;
    savePrefs();
    rearmNotifications(); // respeta el nuevo umbral en la próxima actualización
  });
}

// Estilo de píldora: cambia el indicador y, como cada estilo tiene su propio
// tamaño de ventana, re-aplica el layout para que la píldora se reajuste en vivo
// (también si ahora mismo está colapsada/asomada).
if (el.optPillStyle) {
  el.optPillStyle.addEventListener("change", () => {
    prefs.pillStyle = el.optPillStyle.value;
    savePrefs();
    applyPillStyle();
    // Repinta con el último dato para que el anillo (u otra variante) muestre su
    // relleno actual al instante, sin esperar al próximo sondeo.
    if (ui.lastPayload) render(ui.lastPayload);
    applyLayout();
  });
}

// ---- Colores del borde de la ventana ----
// Cada selector escribe su estado en prefs y re-aplica en vivo (evento `input`,
// así se ve mientras se arrastra el selector). El botón restablece a defaults.
const borderInputs = {
  normal: el.optBorderNormal,
  warn: el.optBorderWarn,
  crit: el.optBorderCrit,
};
for (const [key, input] of Object.entries(borderInputs)) {
  if (!input) continue;
  input.addEventListener("input", () => {
    if (!prefs.borderColors) prefs.borderColors = { normal: null, warn: null, crit: null };
    prefs.borderColors[key] = input.value;
    savePrefs();
    applyBorderColors();
  });
}
if (el.borderReset) {
  el.borderReset.addEventListener("click", () => {
    prefs.borderColors = { normal: null, warn: null, crit: null };
    savePrefs();
    applyBorderColors();
  });
}

// Recordar última posición: al activarlo, captura la posición actual como base.
el.optRememberPos.addEventListener("change", async () => {
  prefs.rememberPosition = el.optRememberPos.checked;
  savePrefs();
  // Al activarlo, captura la posición actual como ancla (esquina + coordenada),
  // el mismo formato que usan el arrastre y el cambio de modo.
  if (prefs.rememberPosition) await captureAnchor();
});

// --- Selector de posición con previsualización en vivo ----
const posCells = el.positionGrid
  ? Array.from(el.positionGrid.querySelectorAll(".pos-cell"))
  : [];

posCells.forEach((cell) => {
  // Cambia la posición SOLO al hacer clic (el hover solo resalta en CSS).
  cell.addEventListener("click", async () => {
    const pos = await anchorPosition(cell.dataset.h, cell.dataset.v, gridTarget());
    if (pos) {
      await setPos(pos);
      prefs.rememberPosition = true;
      el.optRememberPos.checked = true;
      // Guarda el ancla derivada de la posición ya aplicada (formato unificado).
      await captureAnchor();
    }
    posCells.forEach((c) => c.classList.remove("active"));
    cell.classList.add("active");
  });
});

// Abrir con Claude Code: instala/quita el hook SessionStart en settings.json.
el.optAutostart.addEventListener("change", async () => {
  const on = el.optAutostart.checked;
  try {
    await invoke(on ? "install_autostart" : "uninstall_autostart");
  } catch (e) {
    console.error("autostart:", e);
    el.optAutostart.checked = !on; // revertir si falló
  }
});

// Cerrar con Claude Code: instala/quita el hook SessionEnd que cierra el widget
// al salir de la terminal. Quitarlo restaura settings.json a como estaba antes.
el.optClose.addEventListener("change", async () => {
  const on = el.optClose.checked;
  try {
    await invoke(on ? "install_shutdown" : "uninstall_shutdown");
  } catch (e) {
    console.error("shutdown:", e);
    el.optClose.checked = !on; // revertir si falló
  }
});

// Contexto oficial: instala/quita el puente statusLine (envuelve tu statusline).
el.optStatusline.addEventListener("change", async () => {
  const on = el.optStatusline.checked;
  try {
    await invoke(on ? "install_statusline_bridge" : "uninstall_statusline_bridge");
  } catch (e) {
    console.error("statusline bridge:", e);
    el.optStatusline.checked = !on; // revertir si falló
  }
});

// ---- Idioma ----
// Primera opción: "Automático" (vuelve a la detección por SO, prefs.locale=null).
const autoOpt = document.createElement("option");
autoOpt.value = "auto";
autoOpt.textContent = autoLabel();
el.optLang.appendChild(autoOpt);
// La etiqueta "Automático" depende de la tabla del idioma activo: si se cargó
// de forma perezosa, la refrescamos cuando esté lista.
i18nReady.then(() => {
  autoOpt.textContent = autoLabel();
});
// Luego los idiomas soportados (su nombre en su propio idioma). Cada opción
// lleva su `lang` (y `dir` para RTL) para que los lectores de pantalla cambien
// de voz al idioma correcto al recorrer la lista — práctica de GOV.UK/Apple.
for (const [code, name] of Object.entries(SUPPORTED)) {
  const opt = document.createElement("option");
  opt.value = code;
  opt.textContent = name;
  opt.lang = code;
  if (isRTL(code)) opt.dir = "rtl";
  el.optLang.appendChild(opt);
}
// Refleja el estado: "auto" si no hay idioma forzado.
el.optLang.value = prefs.locale || "auto";
// Cambio en caliente: carga la tabla del idioma (si hace falta) y, ya lista,
// re-traduce estático + dinámico, sin reiniciar.
el.optLang.addEventListener("change", async () => {
  const v = el.optLang.value;
  prefs.locale = v === "auto" ? null : v;
  savePrefs();
  await setLocale(prefs.locale ?? detectLocale(null));
  autoOpt.textContent = autoLabel(); // re-traduce la etiqueta "Automático"
  applyStaticI18n();
  updatePinUi();
  if (ui.lastPayload) render(ui.lastPayload);
});

// ---- Navegación de vistas ----
function showSettings(show) {
  settingsOpen = show;
  el.settingsView.classList.toggle("hidden", !show);
  el.metricsView.classList.toggle("hidden", show);
}
// El botón de configuración ahora alterna: abre y cierra.
el.settingsBtn.addEventListener("click", () => showSettings(!settingsOpen));
el.settingsClose.addEventListener("click", () => showSettings(false));

// ---- Refresco manual (con giro fluido) ----
// El giro es FLUIDO (Web Animations API): velocidad constante mientras se
// obtiene el dato y, al terminar, DESACELERA con inercia hasta cerrar una vuelta
// entera nada de congelar el icono a mitad de giro al quitar una clase.
const refreshIco = el.refreshBtn.querySelector(".ico");

// Ángulo actual (grados) de la animación en curso. El navegador expone la
// rotación como una matriz CSS `matrix(a, b, c, d, e, f)`; para una rotación
// pura, (a, b) = (cos θ, sin θ), así que recuperamos θ con atan2(b, a). Lo
// necesitamos para que el frenado por inercia arranque justo donde está el icono.
function currentRotation(node) {
  const tf = getComputedStyle(node).transform;
  const m = tf && tf.startsWith("matrix") ? tf.match(/matrix\(([^)]+)\)/) : null;
  if (!m) return 0;
  const [a, b] = m[1].split(",").map(parseFloat);
  return (Math.atan2(b, a) * 180) / Math.PI;
}

async function fluidSpin(task) {
  const SPEED = 850; // ms por vuelta (constante mientras carga)
  const spin = refreshIco.animate(
    [{ transform: "rotate(0deg)" }, { transform: "rotate(360deg)" }],
    { duration: SPEED, iterations: Infinity, easing: "linear" },
  );
  const started = performance.now();
  try {
    await task();
  } finally {
    // Mínimo de giro visible aunque el dato llegue al instante.
    const elapsed = performance.now() - started;
    if (elapsed < 500) await new Promise((r) => setTimeout(r, 500 - elapsed));

    // Inercia: desde el ángulo actual, cierra la vuelta + una extra con ease-out.
    const from = ((currentRotation(refreshIco) % 360) + 360) % 360;
    const to = 360 - from + 360 + from; // = from + (360-from) + 360
    spin.cancel();
    const ease = refreshIco.animate(
      [{ transform: `rotate(${from}deg)` }, { transform: `rotate(${to}deg)` }],
      { duration: 760, easing: "cubic-bezier(0.16, 1, 0.3, 1)", fill: "forwards" },
    );
    await ease.finished.catch(() => {});
    ease.cancel();
    refreshIco.style.transform = "none";
  }
}

el.refreshBtn.addEventListener("click", async () => {
  if (refreshing) return;
  refreshing = true;
  try {
    await fluidSpin(() => invoke("refresh_plan"));
  } catch (e) {
    console.error("refresh_plan:", e);
  } finally {
    refreshing = false;
  }
});

el.hideBtn.addEventListener("click", () => win.hide());

// ---- Tirador de redimensionado ----
// Al pulsar el agarre de la esquina, window.js conduce el resize con la
// proporción bloqueada y reescala el contenido (texto incluido).
if (el.resizeGrip) {
  el.resizeGrip.addEventListener("pointerdown", startGripResize);
}
