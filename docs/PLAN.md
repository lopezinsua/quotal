# Plan de mejora de Quotal

> Estado: propuesta · Fecha: 2026-07-01 · Versión base: v0.3.2
>
> Documento de trabajo orientado a desarrolladores. El "Roadmap" del README es la
> versión de cara al usuario (features visibles); **este** documento es el plan
> técnico interno: qué tocar, por qué, y en qué orden por impacto/riesgo.
>
> Para entender cómo está construido Quotal (tuberías de datos, el punto de
> extensión `ContextProvider`, el contrato de datos y la superficie IPC), ver
> [ARCHITECTURE.md](ARCHITECTURE.md).

## Principios

- Priorizar **valor con menos riesgo**. PRs pequeños y atómicos, no cambios gigantes.
- **Seguridad primero**: este tool reutiliza el token OAuth de Claude Code y escribe
  en sus ficheros. Cualquier regresión ahí puede romper el login del usuario.
- Reducir el **acoplamiento a internals de Claude Code**; preparar el terreno para
  una futura API oficial (que sería "solo otro provider").

## Valoración honesta del punto de partida

Mucho de lo que parecería "deuda" ya está resuelto y bien resuelto. No tocar:

- **Graceful degradation / fallbacks** — `poller::apply_plan` conserva el último dato
  bueno ante un 429, cae al statusLine offline y solo entonces muestra error.
- **Detección de deriva de formato** — `schema_watch.rs` detecta cuando un contenedor
  conocido pierde sus campos, dedupea, loguea con versión y lo expone a la UI.
- **Manejo del token OAuth** — `usage_api.rs`: single-flight en el refresh,
  compare-and-swap contra Claude Code antes de pisar el fichero, `0600` en Unix,
  recuperación de la carrera de rotación del refresh_token.
- **Code signing (Windows)** — el scaffolding de `release.yml` ya está completo; el
  bloqueo es el **certificado** (coste recurrente), no el código.
- **Updater firmado (minisign), themes, notificaciones, i18n, escritura atómica,
  hooks reversibles** — hechos.

### Decisiones conscientes de NO hacer

- **No migrar el frontend a Svelte/SolidJS.** ~2.500 líneas de JS vanilla ya modular
  (19 ficheros) y testeado. Máximo riesgo de regresión, valor para el usuario ≈ 0.
- **No escribir código nuevo de firma de macOS.** El bloqueo es el certificado/cuenta,
  no el código. Descomentar el bloque de `release.yml` cuando exista la cuenta.

## Puntos débiles reales (los que sí atacamos)

1. **El camino que puede romper la cuenta del usuario no tiene tests de orquestación.**
   Las funciones puras de parseo sí están testeadas, pero `usage_api::fetch()`, la
   carrera de rotación (`recover_with_fresh_creds`) y el CAS de `persist_tokens_file`
   no se prueban contra un servidor HTTP simulado ni con ficheros temporales. Es el
   agujero más grave.
2. **No hay detección proactiva de versión de Claude Code.** `schema_watch` es
   *reactivo* (avisa cuando algo ya se rompió). Falta capturar la versión de CC (viene
   en el JSON del statusLine) y correlacionarla.
3. **El puente de statusLine es frágil ante cambios del usuario.** `install_statusline_bridge`
   incrusta una *foto* del comando `foreign` en el `.cjs`; si el usuario cambia su
   statusLine después de instalar el puente, Quotal sigue ejecutando el comando viejo.
   El wrapper además reintroduce ese comando por shell (`spawnSync(ORIG,{shell:true})`).
4. **`latest_transcript()` escanea recursivamente todo `~/.claude/projects/` por evento**
   y elige por mtime → sesión equivocada con muchos proyectos; el watcher recursivo
   puede agotar los límites de inotify en Linux sin fallback a polling.
5. **No hay modo solo-lectura.** Siendo la confianza el pitch central del proyecto, un
   modo observador que desactive *toda* escritura (write-back del token + hooks) es una
   feature diferencial de bajo riesgo.

## Cambios de mayor impacto / menor riesgo

| # | Cambio | Por qué | Riesgo |
|---|--------|---------|--------|
| 1 | Tests de integración del backend (usage API + watcher) + coverage + `cargo audit` en CI | Blinda el código que puede romper el login de Claude. Mayor ROI del repo | Nulo (solo tests) |
| 2 | Detección proactiva de versión de CC + surfacing del aviso de deriva en la UI | Avisar *antes* de romperse, no después | Bajo |
| 3 | Modo solo-lectura / "observador" | Feature de confianza, refuerza el mensaje de seguridad | Bajo |
| 4 | Robustez del puente statusLine (re-snapshot del `foreign`, endurecer wrapper) | Quita fragilidad ante cambios del usuario | Medio |
| 5 | Abstracción `UsageProvider` (trait + providers) | Desacopla de internals; prepara API oficial futura | Bajo (refactor cubierto por tests de la semana 1) |

## Roadmap de 4 semanas

### Semana 1 — Blindar lo peligroso (testing)
- **PR1** — `httpmock` + `tempfile` como dev-deps; tests de `fetch()` happy-path y
  401→refresh→retry.
- **PR2** — tests de la carrera de rotación (`recover_with_fresh_creds`) y del CAS de
  `persist_tokens_file` con ficheros temporales.
- **PR3** — test de integración del ciclo install/uninstall de los 3 hooks
  (statusline / autostart / shutdown) sobre un `settings.json` temporal, verificando
  que NO toca hooks ajenos.
- **PR4** — CI: `cargo-llvm-cov` con umbral mínimo + `cargo audit` + `npm audit --audit-level=high`.

### Semana 2 — Resiliencia frente a Claude Code
- **PR5** — capturar y persistir la versión de CC desde el statusLine.
- **PR6** — surfacing del aviso de `schema_watch` en la UI (banner no intrusivo + link
  a "actualizar Quotal").
- **PR7** — robustez del puente: re-snapshot del comando `foreign` al arrancar
  (dentro de `resync_installed_hooks`).
- **PR8** — fallback del watcher a modo polling si `notify` falla al registrar el watch.

### Semana 3 — Confianza y desacoplo
- **PR9** — modo solo-lectura (toggle + cortar write-back de token y bloquear hooks).
- **PR10** — refactor a trait `UsageProvider` (sin cambio de comportamiento).
- **PR11** — documentar la API interna (providers + contrato de datos) en `docs/`.

### Semana 4 — Ecosistema y pulido
- **PR12** — `CONTRIBUTING.md` (build, tests, estilo, cómo añadir un provider).
- **PR13** — pulido de la animación pill↔widget (afinar curva/timing).
- **PR14** — empujar el certificado OSS gratis (SignPath / Azure Trusted Signing);
  descomentar firma de macOS solo cuando haya cuenta.

## Seguimiento

Marcar cada PR al completarlo. Mantener este documento como fuente de verdad del plan
técnico; el README solo refleja las features de cara al usuario cuando ya están hechas.
