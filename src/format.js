// format.js — Formateadores y helpers de presentación (sin estado).

import { t, intlLocale } from "./i18n.js";

// Severidad combinada (peor de sesión/semana), por si la API no la manda.
export function worstSeverity(plan) {
  const rank = (s, p) => {
    if (s === "critical" || (p != null && p >= 90)) return 2;
    if (s === "warning" || (p != null && p >= 75)) return 1;
    return 0;
  };
  const r = Math.max(
    rank(plan.session_severity, plan.session_percent),
    rank(plan.weekly_severity, plan.weekly_percent),
  );
  return r === 2 ? "critical" : r === 1 ? "warning" : "normal";
}

// Tamaño de ventana de contexto en forma compacta: 1000000 -> "1M", 200000 -> "200k".
export function fmtWindow(n) {
  if (n == null) return "";
  if (n >= 1e6) return Number.isInteger(n / 1e6) ? `${n / 1e6}M` : `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${Math.round(n / 1e3)}k`;
  return String(n);
}

export function fmtTokens(n) {
  if (n == null) return "—";
  if (n >= 1e9) return (n / 1e9).toFixed(2) + "B";
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + "k";
  return String(n);
}

// Cuenta atrás relativa: "Resets in 3h 55m" (traducido).
export function fmtResetIn(iso) {
  if (!iso) return t("reset_dash");
  const ms = new Date(iso).getTime() - Date.now();
  if (isNaN(ms) || ms <= 0) return t("reset_now");
  const h = Math.floor(ms / 3.6e6);
  const m = Math.floor((ms % 3.6e6) / 6e4);
  return h >= 1 ? t("reset_in_h", { h, m }) : t("reset_in_m", { m });
}

// Fecha absoluta corta, con día/hora formateados según el idioma (Intl).
export function fmtResetAt(iso) {
  if (!iso) return t("reset_dash");
  const d = new Date(iso);
  if (isNaN(d.getTime())) return t("reset_dash");
  const loc = intlLocale();
  const day = d.toLocaleDateString(loc, { weekday: "short" }).replace(".", "");
  const time = d.toLocaleTimeString(loc, { hour: "2-digit", minute: "2-digit" });
  return t("reset_at", { day, time });
}

export function fmtFreshness(secs) {
  if (secs == null || secs >= Number.MAX_SAFE_INTEGER) return t("fresh_none");
  if (secs < 60) return t("fresh_now");
  if (secs < 3600) return t("fresh_min", { n: Math.floor(secs / 60) });
  return t("fresh_hour", { n: Math.floor(secs / 3600) });
}

export function secsSince(iso) {
  if (!iso) return null;
  const ms = new Date(iso).getTime();
  if (isNaN(ms)) return null;
  return Math.max(0, (Date.now() - ms) / 1000);
}
