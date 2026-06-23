// boot.js — Arranque de i18n.
//
// Resuelve el idioma activo (carga perezosa: solo se descarga ese idioma) ANTES
// del primer render. Se expone la promesa `i18nReady` para que el wiring re-
// traduzca la UI cuando la tabla del idioma esté lista. Es un módulo hoja para
// que tanto `main.js` como `controls.js` puedan importar `i18nReady` sin ciclos.

import { setLocale, detectLocale } from "./i18n.js";
import { prefs } from "./prefs.js";

// El primer render usa el inglés incrustado del HTML (cero parpadeo, cero red);
// al resolver la tabla del idioma se re-traduce.
export const i18nReady = setLocale(detectLocale(prefs.locale));
