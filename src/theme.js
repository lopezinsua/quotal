// theme.js — Tema (claro/oscuro) y color de acento del widget. Lógica PURA de
// clases CSS sobre la tarjeta: el aspecto vive en styles.css (.theme-light y
// .accent-*); aquí solo decidimos qué clase lleva #card. Separado de controls.js
// para poder probarlo aislado (igual que notify.js).
//
// Convención: el valor por DEFECTO no lleva clase (lo da el CSS base) — "dark"
// para el tema y "default" para el acento. Así una tarjeta sin personalizar es
// idéntica al diseño original y no arrastra clases muertas.

export const THEMES = ["dark", "light"];
export const DEFAULT_THEME = "dark";

export const ACCENTS = ["default", "green", "blue", "violet", "amber"];
export const DEFAULT_ACCENT = "default";

// Sanea una preferencia a un valor conocido (cae al default si es inválida).
export const theme = (pref) => (THEMES.includes(pref) ? pref : DEFAULT_THEME);
export const accent = (pref) => (ACCENTS.includes(pref) ? pref : DEFAULT_ACCENT);

// Aplica el tema: limpia cualquier theme-* previo y, si no es el oscuro (default),
// añade theme-<tema>. Devuelve el tema saneado (útil para sincronizar el selector).
export function applyTheme(card, pref) {
  const t = theme(pref);
  card.classList.remove(...THEMES.map((x) => `theme-${x}`));
  if (t !== DEFAULT_THEME) card.classList.add(`theme-${t}`);
  return t;
}

// Aplica el acento: limpia cualquier accent-* previo y, si no es "default",
// añade accent-<color>. Devuelve el acento saneado.
export function applyAccent(card, pref) {
  const a = accent(pref);
  card.classList.remove(...ACCENTS.map((x) => `accent-${x}`));
  if (a !== DEFAULT_ACCENT) card.classList.add(`accent-${a}`);
  return a;
}
