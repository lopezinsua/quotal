// update.js — Avisos de la UI: actualización de la app y dependencias del
// sistema. No hace polling: reacciona al evento `update://available` que emite
// el backend al arrancar y a las consultas bajo demanda (botón de ajustes).
//
//   - Actualización: el backend solo INFORMA; aquí mostramos el aviso con tres
//     acciones — Actualizar (instala y reinicia), Descartar (oculta hasta el
//     próximo arranque) y No mostrar más (silencia ESA versión hasta que salga
//     otra, persistido en prefs.dismissedUpdate).
//   - Dependencias (solo Linux): al arrancar preguntamos al backend qué libs
//     nativas faltan; si hay alguna, abrimos el widget y lo avisamos, con el
//     comando exacto para instalarlas.

import { invoke, listen } from "./tauri.js";
import { el } from "./dom.js";
import { prefs, savePrefs } from "./prefs.js";
import { t } from "./i18n.js";
import { ui } from "./state.js";
import { applyLayout } from "./window.js";

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

// ---------------------------------------------------------------------------
// Actualización de la app
// ---------------------------------------------------------------------------

let pendingVersion = null;

function setUpdateBusy(busy) {
  el.updateInstall.disabled = busy;
  el.updateDismiss.disabled = busy;
  el.updateMute.disabled = busy;
}

// `force` ignora el silenciado (lo usa la comprobación manual desde ajustes).
function showUpdate(status, force = false) {
  if (!status || !status.available || !status.version) return;
  if (!force && prefs.dismissedUpdate === status.version) return;
  pendingVersion = status.version;
  el.updateText.textContent = t("upd_available", { v: status.version });
  setUpdateBusy(false);
  show(el.updateBanner, true);
  revealCard();
}

el.updateInstall.addEventListener("click", async () => {
  setUpdateBusy(true);
  el.updateText.textContent = t("upd_installing");
  try {
    // Si todo va bien, el backend reinicia la app y este await nunca resuelve.
    await invoke("update_install");
  } catch (e) {
    el.updateText.textContent = t("upd_failed", { err: String(e) });
    setUpdateBusy(false);
  }
});

// Descartar: oculta el aviso esta vez (reaparece en el próximo arranque).
el.updateDismiss.addEventListener("click", () => show(el.updateBanner, false));

// No mostrar más: recuerda esta versión y no vuelve hasta que salga otra.
el.updateMute.addEventListener("click", () => {
  if (pendingVersion) {
    prefs.dismissedUpdate = pendingVersion;
    savePrefs();
  }
  show(el.updateBanner, false);
});

// Aviso empujado por el backend al arrancar.
listen("update://available", (e) => showUpdate(e.payload));

// Botón "Buscar actualizaciones" (ajustes). La comprobación manual es explícita,
// así que muestra el aviso aunque la versión estuviera silenciada.
if (el.updCheck) {
  el.updCheck.addEventListener("click", async () => {
    el.updCheck.disabled = true;
    el.updStatus.textContent = t("upd_checking");
    try {
      const status = await invoke("update_check");
      if (status.available && status.version) {
        el.updStatus.textContent = t("upd_available", { v: status.version });
        showUpdate(status, true);
      } else if (status.error) {
        el.updStatus.textContent = t("upd_failed", { err: status.error });
      } else {
        el.updStatus.textContent = t("upd_uptodate");
      }
    } catch (e) {
      el.updStatus.textContent = t("upd_failed", { err: String(e) });
    } finally {
      el.updCheck.disabled = false;
    }
  });
}

// Versión instalada, mostrada en ajustes.
invoke("get_config")
  .then((c) => {
    if (el.updCurrent && c && c.version) {
      el.updCurrent.textContent = t("upd_current", { v: c.version });
    }
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
invoke("check_system_deps").then(showDeps).catch(() => {});
