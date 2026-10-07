// release.js — Lógica PURA de las actualizaciones (sin DOM ni IPC): novedades
// de una versión, errores legibles y cuándo avisar de que la app se actualizó.
// update.js la usa para pintar; aquí se puede testear aislada.

import { t } from "./i18n.js";

// Notas de versión (Markdown del `latest.json`/CHANGELOG) → lista de puntos en
// texto plano para pintarla con `textContent` (nunca `innerHTML`: las notas
// vienen de la red). Se queda con los puntos de lista y los párrafos, sin
// títulos ni sintaxis (`**`, `` ` ``, enlaces). Como mucho `max` puntos; si hay
// más, el último indica cuántos quedan fuera.
export function notesItems(md, max = 8) {
  if (!md) return [];
  const clean = (s) =>
    s
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .replace(/[*_`]+/g, "")
      .replace(/\s+/g, " ")
      .trim();
  const items = [];
  for (const raw of String(md).split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || /^#{1,6}\s/.test(line) || /^[-=]{3,}$/.test(line)) continue;
    const bullet = line.match(/^(?:[-*+]|\d+\.)\s+(.*)$/);
    if (bullet) items.push(clean(bullet[1]));
    else if (/^\s{2,}\S/.test(raw) && items.length) {
      items[items.length - 1] += " " + clean(line); // continuación del punto anterior
    } else items.push(clean(line));
  }
  const out = items.filter(Boolean);
  if (out.length <= max) return out;
  return [...out.slice(0, max - 1), t("upd_more", { n: out.length - (max - 1) })];
}

// Error de actualización legible. Los del updater son técnicos ("error sending
// request for url…"); los casos habituales se traducen y el resto se muestra tal
// cual dentro del mensaje genérico.
export function updateErrorText(err) {
  const raw = String(err ?? "");
  if (/signature|minisign|verif/i.test(raw)) return t("upd_err_sig");
  if (/request|connect|dns|resolve|timed? ?out|network|offline|tls|certificate/i.test(raw)) {
    return t("upd_err_net");
  }
  return t("upd_failed", { err: raw });
}

// Decide si toca el aviso de "actualizado" y con qué novedades. PURA. En la
// primera ejecución no hay versión previa → nada (no es una actualización).
export function afterUpdateNotice(lastVersion, current, pendingUpdate) {
  if (!current || !lastVersion || lastVersion === current) return { show: false, notes: null };
  const notes = pendingUpdate && pendingUpdate.version === current ? pendingUpdate.notes : null;
  return { show: true, notes };
}
