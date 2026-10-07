// pace.js — Ritmo de consumo: cálculo PURO (sin DOM ni estado) de cuánto de la
// ventana de uso ha transcurrido y de cuándo se alcanzaría el límite al ritmo
// actual. render.js lo pinta; aquí solo hay aritmética testeable.

/// Duración de cada ventana de uso del plan.
export const SESSION_WINDOW_MS = 5 * 3.6e6;
export const WEEKLY_WINDOW_MS = 7 * 24 * 3.6e6;

// Para proyectar hacen falta muestras que cubran al menos este tramo: con menos,
// una sola respuesta larga dispara la pendiente y la estimación no significa nada.
const MIN_SPAN_MS = 10 * 60 * 1000;
// Solo cuenta el ritmo RECIENTE: lo que gastaste hace horas no predice el ahora.
const RATE_LOOKBACK_MS = 60 * 60 * 1000;

/// Fracción (0..1) de la ventana ya transcurrida, a partir de su instante de
/// reinicio. `null` si no hay reinicio válido. Es la posición de la marca de
/// ritmo: si el uso va por delante de ella, se gasta más rápido de lo sostenible.
export function windowElapsed(resetsAt, windowMs, now = Date.now()) {
  const end = Date.parse(resetsAt);
  if (!Number.isFinite(end) || !(windowMs > 0)) return null;
  const elapsed = 1 - (end - now) / windowMs;
  return Math.min(1, Math.max(0, elapsed));
}

/// Milisegundos hasta llegar al 100 % al ritmo reciente, o `null` si no aplica:
/// sin datos suficientes, uso estable o a la baja, o si al ritmo actual el
/// límite no llegaría antes del reinicio (entonces no hay nada de qué avisar).
///
/// `samples`: [{ t: ms, v: % }] en orden cronológico. Solo se usan las de la
/// ventana ACTUAL (posteriores a su inicio) y de la última hora; la pendiente se
/// ajusta por mínimos cuadrados para no depender de dos puntos sueltos.
export function limitEta(samples, resetsAt, windowMs, now = Date.now()) {
  const end = Date.parse(resetsAt);
  if (!Number.isFinite(end) || end <= now || !Array.isArray(samples)) return null;
  const from = Math.max(end - windowMs, now - RATE_LOOKBACK_MS);
  const pts = samples.filter(
    (s) => Number.isFinite(s.t) && Number.isFinite(s.v) && s.t >= from && s.t <= now,
  );
  if (pts.length < 2 || pts[pts.length - 1].t - pts[0].t < MIN_SPAN_MS) return null;

  // Regresión lineal v = a + b·t (t relativo para no perder precisión).
  const t0 = pts[0].t;
  const n = pts.length;
  let sx = 0, sy = 0, sxx = 0, sxy = 0;
  for (const p of pts) {
    const x = p.t - t0;
    sx += x;
    sy += p.v;
    sxx += x * x;
    sxy += x * p.v;
  }
  const den = n * sxx - sx * sx;
  if (den <= 0) return null;
  const slope = (n * sxy - sx * sy) / den; // % por ms
  if (!(slope > 0)) return null;

  const current = pts[n - 1].v;
  if (current >= 100) return 0;
  const eta = (100 - current) / slope - (now - pts[n - 1].t);
  if (!Number.isFinite(eta)) return null;
  const etaClamped = Math.max(0, eta);
  return now + etaClamped < end ? etaClamped : null;
}
