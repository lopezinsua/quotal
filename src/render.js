// render.js — Pintado del estado en el DOM: ventanas de uso del plan, contexto
// y línea de frescura. No hace I/O: recibe el payload y lo refleja.

import { el, SOURCE_CLASSES, SEVERITY_CLASSES } from "./dom.js";
import { ui } from "./state.js";
import { t } from "./i18n.js";
import { pushSample, series } from "./history.js";
import {
  worstSeverity,
  fmtWindow,
  fmtTokens,
  fmtResetIn,
  fmtResetAt,
  fmtFreshness,
  secsSince,
} from "./format.js";

// Circunferencia del mini-gauge de la píldora (r=15 en su viewBox 36×36).
const PILL_GAUGE_CIRC = 2 * Math.PI * 15;

// Respeta la preferencia del SO de reducir movimiento (sin count-up -> valor seco).
const REDUCED_MOTION =
  typeof window !== "undefined" &&
  window.matchMedia &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// Cuenta el número desde su valor anterior hasta el nuevo con requestAnimationFrame,
// re-renderizando el texto cada frame con `fmt` (ease-out cúbico). Sensación de
// medidor que "sube" en vez de saltar de golpe. El valor previo se guarda en el
// nodo (dataset.val); si no hay (primer dato), aparece directo sin animar. Cancela
// cualquier cuenta en curso sobre el mismo nodo para no solaparlas.
const COUNT_MS = 420;
function countTo(node, to, fmt) {
  if (!node) return;
  const prev = Number(node.dataset.val);
  const from = Number.isFinite(prev) ? prev : to;
  node.dataset.val = String(to);
  if (node._raf) cancelAnimationFrame(node._raf);
  if (REDUCED_MOTION || from === to) {
    node.textContent = fmt(to);
    node._raf = null;
    return;
  }
  const start = performance.now();
  const tick = (now) => {
    const t = Math.min(1, (now - start) / COUNT_MS);
    const e = 1 - Math.pow(1 - t, 3); // ease-out cúbico, mismo lenguaje que el morph
    node.textContent = fmt(from + (to - from) * e);
    node._raf = t < 1 ? requestAnimationFrame(tick) : null;
  };
  node._raf = requestAnimationFrame(tick);
}

// Re-arranca la animación de "cambio de valor" (pulso de color) de forma fiable:
// quitar la clase + forzar reflow + volver a añadirla reinicia el keyframe.
function flash(node) {
  if (!node) return;
  node.classList.remove("flash");
  void node.offsetWidth;
  node.classList.add("flash");
}

// Sparkline (polilínea) del % de sesión a escala fija 0..100. Con menos de dos
// muestras se oculta (clase `.empty`) para no pintar una línea sin sentido.
function renderSparkline(vals) {
  if (!el.sessionSparkPoly || !el.sessionSpark) return;
  if (!vals || vals.length < 2) {
    el.sessionSpark.classList.add("empty");
    el.sessionSparkPoly.setAttribute("points", "");
    return;
  }
  el.sessionSpark.classList.remove("empty");
  const n = vals.length;
  const pts = vals.map((v, i) => {
    const x = (i / (n - 1)) * 100;
    const y = 24 - (Math.min(100, Math.max(0, v)) / 100) * 24;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  });
  el.sessionSparkPoly.setAttribute("points", pts.join(" "));
  // Área bajo la línea, cerrada contra la base, para el relleno degradado.
  if (el.sessionSparkArea) {
    el.sessionSparkArea.setAttribute("points", `${pts.join(" ")} 100,24 0,24`);
  }
}

// Pinta una ventana de uso real (barra + "% usado") a partir del % del servidor.
// Colorea por `severity` de la API (o por umbral si no viene). Devuelve el %.
export function renderUsagePct(fillEl, pctEl, percent, severity) {
  fillEl.classList.remove("warn", "crit");
  if (percent == null) {
    fillEl.style.width = "0%";
    pctEl.textContent = "—";
    delete pctEl.dataset.val; // el próximo valor real aparecerá sin contar desde 0
    return null;
  }
  const pct = Math.min(100, Math.max(0, percent));
  fillEl.style.width = `${pct.toFixed(1)}%`;
  const sev = severity || "";
  if (sev === "critical" || pct >= 90) fillEl.classList.add("crit");
  else if (sev === "warning" || pct >= 75) fillEl.classList.add("warn");
  // Count-up: el "% usado" sube hasta el valor nuevo en vez de saltar.
  countTo(pctEl, pct, (v) => t("used", { n: Math.round(v) }));
  return pct;
}

export function render(p) {
  ui.lastPayload = p;
  const m = p.active;

  // Llegó un payload: salimos del esqueleto de carga inicial.
  el.card.classList.remove("loading");

  el.card.classList.remove(...SOURCE_CLASSES);
  el.card.classList.add(`source-${m.source}`);

  // ---- Ventanas de uso REALES del plan (principal) ----
  const plan = p.plan || {};
  el.planName.textContent = plan.name || t("plan");

  const sessPct = renderUsagePct(
    el.sessionFill,
    el.sessionPct,
    plan.session_percent,
    plan.session_severity,
  );
  el.sessionReset.textContent = plan.session_resets_at
    ? fmtResetIn(plan.session_resets_at)
    : t("reset_dash");

  renderUsagePct(el.weeklyFill, el.weeklyPct, plan.weekly_percent, plan.weekly_severity);
  el.weeklyReset.textContent = plan.weekly_resets_at
    ? fmtResetAt(plan.weekly_resets_at)
    : t("reset_dash");

  // Aviso visual del widget según la peor severidad. La clase también conmuta el
  // token semántico `--state`, del que cuelgan píldora, sparkline y delta.
  el.card.classList.remove(...SEVERITY_CLASSES);
  const sev = worstSeverity(plan);
  if (sev === "critical") el.card.classList.add("sev-critical");
  else if (sev === "warning") el.card.classList.add("sev-warning");

  // Tendencia de la sesión: registra la muestra (solo si es un dato fresco) y
  // repinta sparkline + delta. La animación de cambio se dispara una sola vez.
  const fresh = pushSample(plan.session_percent, plan.fetched_at);
  renderSparkline(series());
  if (fresh && sessPct != null) flash(el.sessionPct);

  // Píldora: los tres estilos se alimentan del MISMO % de sesión y heredan el
  // color de estado vía `--state`. Actualizamos los tres indicadores siempre; el
  // CSS muestra solo el del estilo activo, así cambiar de estilo es instantáneo.
  const pillFrac = sessPct == null ? 0 : Math.min(100, Math.max(0, sessPct)) / 100;
  if (sessPct != null) {
    countTo(el.pillPct, sessPct, (v) => `${Math.round(v)}%`);
  } else {
    el.pillPct.textContent = "—";
    delete el.pillPct.dataset.val;
  }
  // ring: arco del mini-gauge (15% → anillo al 15%).
  if (el.pillGaugeArc) {
    const arc = (pillFrac * PILL_GAUGE_CIRC).toFixed(2);
    el.pillGaugeArc.setAttribute("stroke-dasharray", `${arc} ${PILL_GAUGE_CIRC.toFixed(2)}`);
  }
  // bar: ancho de la barra lineal.
  if (el.pillBarFill) {
    el.pillBarFill.style.width = `${(pillFrac * 100).toFixed(1)}%`;
  }

  // ---- Contexto (secundario). Etiqueta con la ventana REAL del modelo
  // (200k / 1M) cuando la conocemos, en vez de un texto fijo. ----
  el.contextLabel.textContent = m.tokens_limit
    ? `${t("context")} · ${fmtWindow(m.tokens_limit)}`
    : t("context");
  // Capacidad al estilo "KPI card": porcentaje + valor absoluto usado cuando lo
  // tenemos (la ventana total ya va en la etiqueta). Honesto con lo disponible.
  const ctxParts = [];
  if (m.percent_used != null) ctxParts.push(`${m.percent_used.toFixed(0)}%`);
  if (m.tokens_used != null) ctxParts.push(fmtTokens(m.tokens_used));
  el.contextPct.textContent = ctxParts.length ? ctxParts.join(" · ") : "—";

  // ---- Frescura ----
  // Distinguimos tres estados para ser HONESTOS con el dato:
  //  - sin dato           -> "Sin conexión con Claude; <motivo>"
  //  - dato OFFLINE (statusLine, sin red) → lo marcamos como tal
  //  - dato en vivo (online) -> "Última actualización: hace X"
  el.freshness.classList.remove("offline");
  if (plan.available === false) {
    el.freshness.textContent = t("no_conn", { err: plan.error || "—" });
    el.freshness.classList.add("offline");
  } else {
    const fresh = fmtFreshness(secsSince(plan.fetched_at));
    const isOffline = plan.source && plan.source !== "online";
    if (isOffline) {
      el.freshness.textContent = t("offline_sl", { fresh });
      el.freshness.classList.add("offline");
    } else {
      el.freshness.textContent = t("updated", { fresh });
    }
  }

  // ---- Aviso de deriva de esquema (red de seguridad ante cambios de Claude Code) ----
  // El backend manda la lista de fuentes cuyo formato dejó de reconocerse. Si hay
  // alguna, mostramos un aviso discreto invitando a actualizar; si no, lo ocultamos.
  if (el.schemaWarn) {
    const srcs = p.schema_warning;
    if (srcs && srcs.length) {
      el.schemaWarn.textContent = t("schema_warn", { src: srcs.join(", ") });
      el.schemaWarn.classList.remove("hidden");
    } else {
      el.schemaWarn.classList.add("hidden");
    }
  }
}
