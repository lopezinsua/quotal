// update.js — Avisos de la UI: actualización de la app y dependencias del
// sistema. No hace polling: reacciona a los eventos del backend
// (`update://available` al arrancar y cada pocas horas, `update://progress`
// durante la descarga) y a la comprobación manual de Ajustes.
//
//   - Actualización: el backend solo INFORMA; aquí mostramos el aviso con las
//     novedades de la versión y tres acciones — Actualizar (descarga con progreso,
//     instala y reinicia), Descartar (no vuelve a salir en esta sesión) y No
//     mostrar más (silencia ESA versión hasta que salga otra, persistido en
//     prefs.dismissedUpdate).
//   - Tras actualizar: "Actualizado a vX" una sola vez, con las novedades que se
//     guardaron antes de instalar (después del reinicio puede no haber red).
//   - Dependencias (solo Linux): al arrancar preguntamos al backend qué libs
//     nativas faltan; si hay alguna, abrimos el widget y lo avisamos, con el
//     comando exacto para instalarlas.

import { invoke, listen } from "./tauri.js";
import { el } from "./dom.js";
import { prefs, savePrefs, flushPrefs } from "./prefs.js";
import { t } from "./i18n.js";
import { ui } from "./state.js";
import { applyLayout } from "./window.js";
import { i18nReady } from "./boot.js";
import { fmtFreshness, secsSince } from "./format.js";
import { notesItems, updateErrorText, afterUpdateNotice } from "./release.js";

const show = (node, on) => node && node.classList.toggle("hidden", !on);

// Si el widget está colapsado en píldora, lo desplegamos una vez (sin guardar la
// preferencia) para que un aviso importante sea visible. Lo pidió el usuario
// para las dependencias; también ayuda a no perderse una actualización.
function revealCard() {
  if (prefs.collapsed && !ui.peeking) {
    ui.peeking = true;
    applyLayout();
  }
}

// Pinta las novedades (texto plano, nunca HTML: vienen de la red) en `list`.
// Devuelve cuántos puntos hay (0 = no hay notas que enseñar).
function fillNotes(list, notes) {
  const items = notesItems(notes);
  list.replaceChildren(
    ...items.map((text) => {
      const li = document.createElement("li");
      li.textContent = text;
      return li;
    }),
  );
  return items.length;
}

// Despliega/pliega el panel de un aviso compacto (gira el ⌄).
function setExpanded(toggle, panel, open) {
  toggle.setAttribute("aria-expanded", String(open));
  show(panel, open);
}

// Los avisos compactos ocupan el sitio de la cabecera del plan (alto fijo de la
// tarjeta): la clase `has-notice` la oculta mientras alguno esté visible.
function syncNoticeSlot() {
  const visible = [el.updateBanner, el.updatedBanner].some(
    (b) => b && !b.classList.contains("hidden"),
  );
  el.card.classList.toggle("has-notice", visible);
}

// ---------------------------------------------------------------------------
// Actualización de la app
// ---------------------------------------------------------------------------

let pending = null; // { version, notes } del aviso visible
const announced = new Set(); // versiones ya avisadas en esta sesión (no re-desplegar)
const dismissed = new Set(); // "Descartar": no repetir en esta sesión

function setUpdateBusy(busy) {
  // Mientras descarga/instala, el botón sobra (y quita sitio al progreso); si
  // falla, vuelve para reintentar.
  show(el.updateInstall, !busy);
  el.updateInstall.disabled = busy;
  el.updateDismiss.disabled = busy;
  el.updateMute.disabled = busy;
}

function setUpdateText(text, { error = false, title = null } = {}) {
  el.updateText.textContent = text;
  if (title) el.updateText.title = title;
  else el.updateText.removeAttribute("title");
  el.updateBanner.classList.toggle("error", error);
}

// `force` ignora los silenciados (lo usa la comprobación manual desde ajustes).
function showUpdate(status, force = false) {
  if (!status || !status.available || !status.version) return;
  const v = status.version;
  if (!force && (prefs.dismissedUpdate === v || dismissed.has(v))) return;
  pending = { version: v, notes: status.notes || null };
  // Texto corto (la fila es estrecha); el largo queda en el tooltip.
  setUpdateText(t("upd_short", { v }), { title: t("upd_available", { v }) });
  const n = fillNotes(el.updateNotes, pending.notes);
  show(el.updateNotes, n > 0);
  el.updateNotesToggle.title = t(n > 0 ? "upd_whats_new" : "upd_options");
  setExpanded(el.updateNotesToggle, el.updateMore, false);
  show(el.updateProgress, false);
  setUpdateBusy(false);
  show(el.updateBanner, true);
  syncNoticeSlot();
  // Solo se despliega la píldora la PRIMERA vez que se avisa de una versión: la
  // comprobación periódica no debe abrir el widget cada pocas horas.
  if (force || !announced.has(v)) revealCard();
  announced.add(v);
}

el.updateNotesToggle.addEventListener("click", () => {
  const open = el.updateNotesToggle.getAttribute("aria-expanded") !== "true";
  setExpanded(el.updateNotesToggle, el.updateMore, open);
});

// Progreso de la descarga: porcentaje si el servidor anuncia el tamaño; si no,
// los MB recibidos. Al terminar la descarga pasa a "Actualizando…".
listen("update://progress", ({ payload: p }) => {
  if (!p) return;
  show(el.updateProgress, true);
  if (p.phase === "install") {
    setUpdateText(t("upd_installing"));
    el.updateProgressFill.style.width = "100%";
    return;
  }
  if (p.total) {
    const pct = Math.min(100, Math.round((p.downloaded / p.total) * 100));
    setUpdateText(t("upd_downloading", { pct }));
    el.updateProgressFill.style.width = `${pct}%`;
  } else {
    const mb = (p.downloaded / 1048576).toFixed(1);
    setUpdateText(t("upd_downloading_mb", { mb }));
    el.updateProgressFill.style.width = "100%";
  }
});

el.updateInstall.addEventListener("click", async () => {
  setUpdateBusy(true);
  setExpanded(el.updateNotesToggle, el.updateMore, false);
  setUpdateText(t("upd_installing"));
  // Guardamos las novedades ANTES de instalar: tras el reinicio se muestran en el
  // aviso "Actualizado a vX". Y vaciamos los ajustes pendientes: el reinicio del
  // instalador no pasa por `pagehide`.
  if (pending) prefs.pendingUpdate = pending;
  flushPrefs();
  try {
    // Si todo va bien, el backend reinicia la app y este await nunca resuelve.
    await invoke("update_install");
  } catch (e) {
    setUpdateText(updateErrorText(e), { error: true, title: String(e) });
    show(el.updateProgress, false);
    prefs.pendingUpdate = null;
    savePrefs();
    setUpdateBusy(false);
  }
});

// Descartar: oculta el aviso de esta versión durante la sesión (la comprobación
// periódica no lo vuelve a sacar; reaparece en el próximo arranque).
el.updateDismiss.addEventListener("click", () => {
  if (pending) dismissed.add(pending.version);
  show(el.updateBanner, false);
  syncNoticeSlot();
});

// No mostrar más: recuerda esta versión y no vuelve hasta que salga otra.
el.updateMute.addEventListener("click", () => {
  if (pending) {
    prefs.dismissedUpdate = pending.version;
    savePrefs();
  }
  show(el.updateBanner, false);
  syncNoticeSlot();
});

// Aviso empujado por el backend (al arrancar y en cada comprobación periódica).
listen("update://available", (e) => showUpdate(e.payload));

// "Última comprobación: hace X" bajo el botón de Ajustes.
function showLastChecked(iso) {
  if (!el.updStatus) return;
  el.updStatus.textContent = iso ? t("upd_last", { fresh: fmtFreshness(secsSince(iso)) }) : "";
}

// Botón "Buscar actualizaciones" (ajustes). La comprobación manual es explícita,
// así que muestra el aviso aunque la versión estuviera silenciada.
if (el.updCheck) {
  el.updCheck.addEventListener("click", async () => {
    el.updCheck.disabled = true;
    el.updStatus.textContent = t("upd_checking");
    el.updStatus.removeAttribute("title");
    try {
      const status = await invoke("update_check");
      if (status.available && status.version) {
        el.updStatus.textContent = t("upd_available", { v: status.version });
        showUpdate(status, true);
      } else if (status.error) {
        el.updStatus.textContent = updateErrorText(status.error);
        el.updStatus.title = status.error;
      } else {
        el.updStatus.textContent = t("upd_uptodate");
      }
    } catch (e) {
      el.updStatus.textContent = updateErrorText(e);
      el.updStatus.title = String(e);
    } finally {
      el.updCheck.disabled = false;
    }
  });
}

// Comprobación automática (on/off). Vive en el backend, que es quien la hace.
if (el.optAutoUpdate) {
  Promise.all([invoke("update_prefs"), i18nReady])
    .then(([p]) => {
      el.optAutoUpdate.checked = !!(p && p.auto_check);
      showLastChecked(p && p.last_checked);
    })
    .catch(() => {});
  el.optAutoUpdate.addEventListener("change", () => {
    invoke("set_auto_update_check", { enabled: el.optAutoUpdate.checked }).catch((e) => {
      console.error("set_auto_update_check:", e);
      el.optAutoUpdate.checked = !el.optAutoUpdate.checked; // revertir si falló
    });
  });
}

// El aviso de DERIVA de esquema es accionable: la causa típica es que Claude Code
// se actualizó a un formato más nuevo que el que soporta esta versión de Quotal,
// así que al pulsar el banner comprobamos si hay actualización de Quotal. Si la
// hay, mostramos el aviso rico (con botón de instalar); si no, un estado breve.
if (el.schemaWarn) {
  const checkFromDrift = async () => {
    if (el.schemaWarn.dataset.checking === "1") return; // evita dobles clics
    el.schemaWarn.dataset.checking = "1";
    const original = el.schemaWarn.textContent;
    el.schemaWarn.textContent = t("upd_checking");
    const restore = () =>
      setTimeout(() => {
        el.schemaWarn.textContent = original;
      }, 2500);
    try {
      const status = await invoke("update_check");
      if (status && status.available && status.version) {
        el.schemaWarn.textContent = original;
        showUpdate(status, true);
      } else {
        el.schemaWarn.textContent =
          status && status.error ? updateErrorText(status.error) : t("upd_uptodate");
        restore();
      }
    } catch (e) {
      el.schemaWarn.textContent = updateErrorText(e);
      restore();
    } finally {
      delete el.schemaWarn.dataset.checking;
    }
  };
  el.schemaWarn.addEventListener("click", checkFromDrift);
  el.schemaWarn.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      checkFromDrift();
    }
  });
}

// ---------------------------------------------------------------------------
// Tras actualizar: "Actualizado a vX" (una vez) con sus novedades
// ---------------------------------------------------------------------------

// El aviso se cierra solo pasado un rato, salvo que el usuario lo despliegue.
const UPDATED_AUTOCLOSE_MS = 20000;
let updatedTimer = null;
function closeUpdated() {
  clearTimeout(updatedTimer);
  show(el.updatedBanner, false);
  syncNoticeSlot();
}

el.updatedNotesToggle.addEventListener("click", () => {
  clearTimeout(updatedTimer); // lo está leyendo: ya no se cierra solo
  const open = el.updatedNotesToggle.getAttribute("aria-expanded") !== "true";
  setExpanded(el.updatedNotesToggle, el.updatedNotes, open);
});
el.updatedDismiss.addEventListener("click", closeUpdated);

// Versión instalada, mostrada en ajustes; y aviso de "actualizado" si cambió.
// Se espera a la tabla del idioma: `get_config` responde antes de que cargue y
// los textos salían en inglés.
Promise.all([invoke("get_config"), i18nReady])
  .then(([c]) => {
    const version = c && c.version;
    if (!version) return;
    if (el.updCurrent) el.updCurrent.textContent = t("upd_current", { v: version });
    const notice = afterUpdateNotice(prefs.lastVersion, version, prefs.pendingUpdate);
    if (notice.show) {
      el.updatedText.textContent = t("upd_done", { v: version });
      const n = fillNotes(el.updatedNotes, notice.notes);
      show(el.updatedNotesToggle, n > 0);
      setExpanded(el.updatedNotesToggle, el.updatedNotes, false);
      show(el.updatedBanner, true);
      syncNoticeSlot();
      updatedTimer = setTimeout(closeUpdated, UPDATED_AUTOCLOSE_MS);
    }
    prefs.lastVersion = version;
    prefs.pendingUpdate = null;
    savePrefs();
  })
  .catch(() => {});

// ---------------------------------------------------------------------------
// Dependencias del sistema (Linux)
// ---------------------------------------------------------------------------

function showDeps(report) {
  if (!report || !Array.isArray(report.missing) || report.missing.length === 0) return;
  el.depsText.textContent = t("deps_missing", { n: report.missing.length });
  el.depsList.innerHTML = "";
  for (const d of report.missing) {
    const li = document.createElement("li");
    li.textContent = `${d.name} — ${d.package}`;
    el.depsList.appendChild(li);
  }
  el.depsCmd.textContent = report.install_hint || "";
  // El comando de instalación se puede seleccionar/copiar (el resto de la UI no).
  el.depsCmd.style.userSelect = "text";
  show(el.depsDetail, false);
  show(el.depsBanner, true);
  revealCard();
}

el.depsToggle.addEventListener("click", () => {
  el.depsDetail.classList.toggle("hidden");
});

el.depsDismiss.addEventListener("click", () => show(el.depsBanner, false));

el.depsCopy.addEventListener("click", async () => {
  const cmd = el.depsCmd.textContent || "";
  if (!cmd) return;
  try {
    await navigator.clipboard.writeText(cmd);
  } catch {
    // Fallback: selección + execCommand para webviews sin Clipboard API.
    const r = document.createRange();
    r.selectNodeContents(el.depsCmd);
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(r);
    try {
      document.execCommand("copy");
    } catch {
      /* sin portapapeles: el comando queda visible para copiarlo a mano */
    }
  }
  el.depsCopy.textContent = t("deps_copied");
  setTimeout(() => {
    el.depsCopy.textContent = t("deps_copy");
  }, 1500);
});

// Comprobación de dependencias al arrancar (no aplica fuera de Linux: devuelve
// lista vacía y no se muestra nada).
// (tras cargar la tabla del idioma, por la misma razón que arriba)
Promise.all([invoke("check_system_deps"), i18nReady])
  .then(([report]) => showDeps(report))
  .catch(() => {});
