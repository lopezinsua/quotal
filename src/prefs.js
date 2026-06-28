// prefs.js — Preferencias de presentación (persisten en localStorage) y tamaños
// de ventana. Es un singleton: el objeto `prefs` se importa y se muta en sitio.

export const PREFS_KEY = "widget-prefs";

// Tamaño de ventana en modo completo.
export const SIZE_FULL_DEFAULT = { w: 248, h: 268 };

// Tamaño de la ventana-píldora SEGÚN su estilo. Cada variante respira distinto:
//  - bar    -> número + barra lineal, necesita algo de ancho.
//  - ring   -> anillo (logo) + número, compacta.
//  - minimal-> punto de estado + número, la más estrecha.
// La forma es siempre de píldora (bordes 999px); aquí solo varía el lienzo.
export const PILL_SIZES = {
  bar: { w: 98, h: 24 },
  ring: { w: 74, h: 24 },
  minimal: { w: 58, h: 22 },
};
export const PILL_STYLES = Object.keys(PILL_SIZES);
export const DEFAULT_PILL_STYLE = "bar";

// Colores del BORDE de la ventana, por estado. `null` en una clave = usar el
// valor por defecto del CSS (no se inyecta variable). Estos hex son solo el
// valor que muestran los selectores de color cuando no hay personalización; el
// aspecto real "sin tocar" lo siguen dando las variables --border-* del CSS
// (el normal es un hairline translúcido, no un sólido).
export const DEFAULT_BORDER_COLORS = {
  normal: "#3a3a40",
  warn: "#ffc533",
  crit: "#ff6161",
};

export const prefs = Object.assign(
  {
    translucent: false,
    onTop: true,
    collapsed: false,
    showContext: true,
    // Estilo de la píldora colapsada: "bar" | "ring" | "minimal".
    pillStyle: DEFAULT_PILL_STYLE,
    // Colores del borde por estado. Cada clave: hex personalizado o null (=CSS).
    borderColors: { normal: null, warn: null, crit: null },
    // Iluminar el borde de la ventana según la severidad (aviso/crítico)?
    // Si es false, el borde queda neutro (la píldora/sparkline siguen con color).
    borderGlow: true,
    // Mantener el icono de la bandeja en un color fijo (sin cambiar por severidad)?
    trayStaticColor: false,
    // Tamaño elegido por el usuario al redimensionar a mano (modo completo).
    fullSize: null,
    // Posición de la ventana (coords. FÍSICAS). null -> arriba a la derecha.
    rememberPosition: true,
    position: null,
    // Idioma forzado por el usuario; null -> autodetección del SO.
    locale: null,
    // Versión de actualización silenciada con "No mostrar más". Mientras la
    // versión disponible sea ESTA, no se muestra el aviso; al salir una más
    // nueva, vuelve a aparecer.
    dismissedUpdate: null,
  },
  JSON.parse(localStorage.getItem(PREFS_KEY) || "{}"),
);

export const savePrefs = () =>
  localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));

// Tamaño "completo" efectivo: el que el usuario dejó al redimensionar, o el default.
export const fullSize = () => prefs.fullSize || SIZE_FULL_DEFAULT;

// Estilo de píldora vigente (saneado a uno válido).
export const pillStyle = () =>
  PILL_SIZES[prefs.pillStyle] ? prefs.pillStyle : DEFAULT_PILL_STYLE;

// Tamaño de la ventana-píldora para el estilo activo.
export const pillSize = () => PILL_SIZES[pillStyle()];
