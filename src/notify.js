// notify.js — Notificaciones de escritorio al cruzar el umbral de uso.
//
// No hace polling propio: se alimenta del MISMO flujo de plan que pinta la UI
// (`usage://metrics-updated`), así que el coste es nulo. Vigila la SESIÓN (5h) y
// la SEMANA (7d) por separado y avisa una sola vez por ventana al cruzar el
// umbral (`prefs.notifyThreshold`). Anti-spam con detección de FLANCO:
//   - Solo dispara al pasar de "por debajo" a "por encima" del umbral.
//   - Se RE-ARMA cuando la ventana se resetea (cambia su `resets_at`) o el % cae
//     por debajo del umbral.
//   - No molesta en el PRIMER dato: si al abrir ya estás por encima, se marca
//     como ya avisado (baseline) y se esperará al próximo reset para volver a
//     avisar. Evita un aviso en cada arranque.

import { prefs } from "./prefs.js";
import { t } from "./i18n.js";

// API global del plugin (withGlobalTauri). Puede no existir en builds antiguos o
// en tests: lo tratamos como opcional para no romper el resto del frontend.
const api = (window.__TAURI__ && window.__TAURI__.notification) || {};

// Estado de flanco por ventana. `windowKey` = `resets_at` (cambia al resetear);
// `notified` = ya avisamos en ESTA ventana.
const tracked = {
  session: { notified: false, windowKey: null },
  weekly: { notified: false, windowKey: null },
};
// Hasta ver el primer payload no disparamos (evita avisar en cada arranque).
let baselineSet = false;

/// ¿Hay permiso del SO para notificar? Lo pide si aún no se ha decidido.
/// Devuelve `true` si quedó concedido. Tolerante: si la API no está, `false`.
export async function ensureNotifyPermission() {
  if (!api.isPermissionGranted || !api.requestPermission) return false;
  try {
    let granted = await api.isPermissionGranted();
    if (!granted) {
      const res = await api.requestPermission();
      granted = res === "granted";
    }
    return granted;
  } catch (e) {
    console.error("notify permission:", e);
    return false;
  }
}

/// Re-arma los avisos (p. ej. al cambiar el umbral): la próxima evaluación
/// volverá a disparar si ya se está por encima del nuevo umbral.
export function rearmNotifications() {
  tracked.session.notified = false;
  tracked.weekly.notified = false;
}

function send(body) {
  if (!api.sendNotification) return;
  try {
    api.sendNotification({ title: t("notify_title"), body });
  } catch (e) {
    console.error("sendNotification:", e);
  }
}

// Evalúa una ventana (sesión/semana). El SEGUIMIENTO de flanco corre SIEMPRE
// (aunque el aviso esté desactivado): así el estado "ya estás por encima" se
// mantiene al día y activar el aviso más tarde no produce un disparo retroactivo.
// Solo el ENVÍO se condiciona a `prefs.notifyEnabled`.
function evaluate(kind, percent, windowKey, threshold, bodyKey) {
  if (typeof percent !== "number") return;
  const st = tracked[kind];

  // Ventana nueva (reseteó): re-arma el aviso.
  if (windowKey != null && windowKey !== st.windowKey) {
    st.windowKey = windowKey;
    st.notified = false;
  }

  const over = percent >= threshold;
  // Por debajo del umbral, re-arma (permite volver a avisar si vuelve a subir).
  if (!over) {
    st.notified = false;
    return;
  }
  // En el primer payload solo fijamos baseline: si al abrir ya estás por encima,
  // no avisamos hasta el próximo reset.
  if (!baselineSet) {
    st.notified = true;
    return;
  }
  if (!st.notified) {
    st.notified = true;
    if (prefs.notifyEnabled) send(t(bodyKey, { pct: Math.round(percent) }));
  }
}

/// Punto de entrada: recibe el payload de métricas y actualiza el estado de
/// flanco (y notifica si procede). Llamar en cada actualización de plan.
export function notifyFromPayload(payload) {
  const plan = payload && payload.plan;
  // Sin plan utilizable, no tocamos el baseline (esperamos a un dato real).
  if (!plan || !plan.available) return;

  const threshold = Number(prefs.notifyThreshold) || 90;
  evaluate("session", plan.session_percent, plan.session_resets_at, threshold, "notify_session");
  evaluate("weekly", plan.weekly_percent, plan.weekly_resets_at, threshold, "notify_weekly");

  // El baseline se fija con el primer plan real, esté activado o no el aviso.
  baselineSet = true;
}
