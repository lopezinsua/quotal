// i18n.js — Internacionalización ligera con carga perezosa.
//
// Inglés es el idioma FUENTE: va incrustado aquí (`BASE`). Cumple dos papeles
// que conviene NO delegar a la red:
//   1. Primer render sin parpadeo y sin esperar a ningún `fetch`.
//   2. Fallback GARANTIZADO: si una clave falta en otro idioma o si su archivo
//      no carga  siempre hay texto. El fallback no puede depender de I/O.
//
// Los demás idiomas viven en `locales/<código>.json` y se cargan SOLO cuando se
// usan: descargas el peso de UN idioma, no de los once. Cada archivo define las
// mismas claves que `BASE` más `_auto` (la etiqueta "Automático" traducida).
//
// Cadenas estáticas del HTML -> atributos `data-i18n` / `data-i18n-title`.
// Cadenas dinámicas → `t(key, vars)` con sustitución `{x}`. `ar` es RTL.

export const SUPPORTED = {
  en: "English",
  es: "Español",
  zh: "中文",
  hi: "हिन्दी",
  ar: "العربية",
  pt: "Português",
  fr: "Français",
  de: "Deutsch",
  ja: "日本語",
  ru: "Русский",
  ko: "한국어",
};

const RTL = new Set(["ar"]);

// Idioma fuente (inglés). `_auto` es la etiqueta de la opción "Automático".
const BASE = {
  title_pill: "Claude usage — hover to view",
  t_refresh: "Refresh now",
  t_pin: "Pin / unpin",
  t_settings: "Settings",
  t_hide: "Hide to tray",
  t_resize: "Drag to resize",
  plan_limits: "Plan usage limits",
  session: "Current session",
  weekly: "Weekly · all models",
  context: "Context",
  settings_title: "Settings",
  opt_translucent: "Translucent mode",
  opt_ontop: "Always on top",
  opt_pill: "Pill mode (compact)",
  opt_pill_style: "Pill style",
  pill_bar: "Bar",
  pill_ring: "Ring",
  pill_minimal: "Minimal",
  opt_theme: "Theme",
  theme_dark: "Dark",
  theme_light: "Light",
  opt_accent: "Accent color",
  accent_default: "Default",
  accent_green: "Green",
  accent_blue: "Blue",
  accent_violet: "Violet",
  accent_amber: "Amber",
  opt_show_context: "Show context window",
  opt_border_glow: "Light up border by usage",
  opt_tray_static: "Fixed tray color",
  border_colors: "Window border colors",
  border_normal: "Normal",
  border_warn: "Warning",
  border_crit: "Critical",
  border_reset: "Reset to defaults",
  opt_remember_pos: "Remember last position",
  pos_label: "Screen position",
  pos_hint: "(click to move)",
  opt_autostart: "Open with Claude Code",
  opt_close: "Close with Claude Code",
  opt_statusline: "Official context (statusLine)",
  opt_notify: "Notify me near the limit",
  opt_notify_at: "Notify at",
  notify_title: "Quotal — usage alert",
  notify_session: "Your Claude session is at {pct}% used.",
  notify_weekly: "Your weekly Claude limit is at {pct}% used.",
  language: "Language",
  note: "Plan %, usage and reset times come live from Claude (same as /usage). “Open with Claude Code”, “Close with Claude Code” and “Official context” modify your Claude Code settings and are reversible.",
  back: "Back",
  pos_tl: "Top left", pos_tc: "Top center", pos_tr: "Top right",
  pos_ml: "Center left", pos_mc: "Center", pos_mr: "Center right",
  pos_bl: "Bottom left", pos_bc: "Bottom center", pos_br: "Bottom right",
  used: "{n}% used",
  t_spark: "Recent session usage trend",
  reset_in_h: "Resets in {h}h {m}m",
  reset_in_m: "Resets in {m}m",
  reset_now: "Resets now",
  reset_dash: "Resets —",
  reset_at: "Resets {day}, {time}",
  fresh_now: "just now",
  fresh_min: "{n} min ago",
  fresh_hour: "{n}h ago",
  fresh_none: "no data",
  updated: "Updated: {fresh}",
  offline_sl: "Offline · statusLine data · {fresh}",
  no_conn: "No connection to Claude · {err}",
  pin_release: "Release (pill mode)",
  pin_fix: "Pin open",
  plan: "Plan",
  schema_warn: "⚠ Quotal may not be reading Claude Code correctly (changed: {src}). Check for an update.",
  upd_available: "Update available: v{v}",
  upd_install: "Update",
  upd_dismiss: "Dismiss",
  upd_mute: "Don't show again",
  upd_installing: "Updating…",
  upd_failed: "Update failed: {err}",
  upd_section: "Updates",
  upd_check: "Check for updates",
  upd_checking: "Checking…",
  upd_uptodate: "You're up to date",
  upd_current: "Installed: v{v}",
  deps_missing: "{n} missing system dependencies",
  deps_view: "See which",
  deps_hint: "Install them with:",
  deps_copy: "Copy command",
  deps_copied: "Copied",
  _auto: "Automatic",
};

// Tablas ya cargadas. `en` siempre presente (es el fallback incrustado).
const loaded = { en: BASE };
// Fetch en vuelo por idioma: evita lanzar dos descargas del mismo a la vez.
const inflight = {};

// Idioma actual (resuelto). Por defecto inglés.
let current = "en";

/// Mapea un código de idioma del navegador a uno soportado (por subetiqueta).
function resolve(loc) {
  if (!loc) return null;
  const base = String(loc).toLowerCase().split("-")[0];
  return SUPPORTED[base] ? base : null;
}

/// Detecta el idioma: preferencia guardada -> idioma del SO -> inglés.
export function detectLocale(saved) {
  if (saved && SUPPORTED[saved]) return saved;
  const langs = navigator.languages && navigator.languages.length
    ? navigator.languages
    : [navigator.language];
  for (const l of langs) {
    const r = resolve(l);
    if (r) return r;
  }
  return "en";
}

export function getLocale() {
  return current;
}

/// El idioma se escribe de derecha a izquierda? (fuente única para la UI).
export function isRTL(code) {
  return RTL.has(code);
}

/// Devuelve el código BCP-47 para Intl (fechas/horas) según el idioma actual.
export function intlLocale() {
  return current === "zh" ? "zh-CN" : current;
}

/// Carga (perezosa y cacheada) la tabla de un idioma. `en` y los ya cargados
/// resuelven al instante. Si el `fetch` o el JSON fallan, cae a `en` SIN romper:
/// la app sigue funcionando en inglés en vez de quedarse sin textos.
function load(code) {
  if (loaded[code]) return Promise.resolve(loaded[code]);
  if (!SUPPORTED[code] || code === "en") return Promise.resolve(BASE);
  if (!inflight[code]) {
    const url = new URL(`./locales/${code}.json`, import.meta.url);
    inflight[code] = fetch(url)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((dict) => {
        // Mezclamos sobre BASE: aunque al archivo le falte alguna clave, nunca
        // habrá huecos (el fallback ya está dentro de la propia tabla).
        loaded[code] = { ...BASE, ...dict };
        return loaded[code];
      })
      .catch((e) => {
        console.warn(`i18n: no se pudo cargar '${code}', se usa inglés:`, e);
        return BASE;
      })
      .finally(() => {
        delete inflight[code];
      });
  }
  return inflight[code];
}

/// Fija el idioma (cargando su tabla si hace falta) y ajusta la dirección y el
/// `lang` del documento (RTL para árabe). Asíncrona: resuelve cuando la tabla
/// del idioma está disponible, momento idóneo para re-traducir la UI.
export async function setLocale(loc) {
  const code = SUPPORTED[loc] ? loc : "en";
  await load(code);
  current = code;
  document.documentElement.lang = code;
  document.documentElement.dir = RTL.has(code) ? "rtl" : "ltr";
  return code;
}

/// Texto "Automático" en el idioma activo (cae a inglés).
export function autoLabel() {
  return (loaded[current] || BASE)._auto || BASE._auto;
}

/// Traduce una clave con sustitución de variables `{x}`. Cae a inglés.
export function t(key, vars) {
  const table = loaded[current] || BASE;
  let s = table[key];
  if (s == null) s = BASE[key];
  if (s == null) return key;
  if (vars) {
    for (const k in vars) s = s.replaceAll(`{${k}}`, String(vars[k]));
  }
  return s;
}

/// Aplica las traducciones a los nodos estáticos del HTML marcados con
/// `data-i18n` (textContent) y `data-i18n-title` (atributo title).
export function applyStaticI18n(root = document) {
  root.querySelectorAll("[data-i18n]").forEach((el) => {
    el.textContent = t(el.getAttribute("data-i18n"));
  });
  root.querySelectorAll("[data-i18n-title]").forEach((el) => {
    // `setAttribute` en vez de `el.title =`: la propiedad `title` no existe en
    // los elementos SVG (no refleja al atributo), así el tooltip del sparkline
    // tampoco aparecería. El atributo funciona en HTML y SVG por igual.
    el.setAttribute("title", t(el.getAttribute("data-i18n-title")));
  });
}
