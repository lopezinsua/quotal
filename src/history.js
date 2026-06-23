// history.js — Serie temporal corta del % de sesión, para el sparkline y el
// delta de tendencia. Persiste en localStorage para sobrevivir reinicios.
//
// Deduplica por la marca de obtención del backend (`fetched_at`): el re-pintado
// periódico cada 30 s (y los cambios de idioma) NO crean muestras nuevas solo
// las añade un dato realmente fresco. Así el sparkline refleja consumo real y la
// animación de cambio de valor no se dispara en falso.

const KEY = "widget-usage-history";
const MAX = 40; // muestras conservadas (ancho del sparkline)

function load() {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) || "[]");
    return Array.isArray(raw) ? raw.slice(-MAX) : [];
  } catch {
    return [];
  }
}

let samples = load();
let lastStamp = samples.length ? samples[samples.length - 1].t : null;

// Registra una muestra si su marca es nueva. Devuelve `true` solo cuando se
// añadió (dato fresco), para que la UI anime el cambio una única vez.
export function pushSample(percent, stamp) {
  if (percent == null) return false;
  const key = stamp || String(Date.now());
  if (key === lastStamp) return false; // mismo dato re-pintado: no duplicar
  samples.push({ t: key, v: Math.min(100, Math.max(0, percent)) });
  if (samples.length > MAX) samples = samples.slice(-MAX);
  lastStamp = key;
  try {
    localStorage.setItem(KEY, JSON.stringify(samples));
  } catch {
    /* cuota llena o storage no disponible: el sparkline sigue en memoria */
  }
  return true;
}

// Valores en orden cronológico (0..100).
export const series = () => samples.map((s) => s.v);
