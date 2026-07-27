// kimi_usage.rs — Datos REALES de límites de uso de Kimi (Kimi Code CLI de
// Moonshot AI), espejo de `usage_api.rs` (Claude).
//
// Flujo:
//   1. Lee el token OAuth local de `~/.kimi-code/credentials/kimi-code.json` (lo
//      genera y mantiene Kimi Code CLI; nosotros lo reutilizamos, nunca lo creamos).
//   2. El token caduca a los 15 minutos (`expires_in: 900`), así que el refresco es
//      OBLIGATORIO: POST form-url-encoded a `https://auth.kimi.com/api/oauth/token`.
//      Rota AMBOS tokens y el fichero se reescribe FUSIONANDO (para no perder
//      campos), salvo en modo solo-lectura.
//   3. GET `https://api.kimi.com/coding/v1/usages` -> membresía, cuota semanal y
//      ventana rodante de 5h. OJO: los números vienen como STRINGS y `expires_at`
//      del fichero de credenciales está en SEGUNDOS (Claude usa ms).
//
// No inventa límites: el % se deriva de used/limit del servidor de Kimi.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const KIMI_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const DEFAULT_TOKEN_URL: &str = "https://auth.kimi.com/api/oauth/token";
const DEFAULT_USAGE_URL: &str = "https://api.kimi.com/coding/v1/usages";
const USER_AGENT: &str = concat!("quotal/", env!("CARGO_PKG_VERSION"));

/// Endpoint de refresco de token. En producción es el de Kimi; los tests lo
/// redirigen a un servidor local con `QUOTAL_KIMI_TOKEN_URL` (mismo patrón que
/// los `QUOTAL_*_URL` de `usage_api.rs`). Cero cambio en producción.
fn token_url() -> String {
    std::env::var("QUOTAL_KIMI_TOKEN_URL").unwrap_or_else(|_| DEFAULT_TOKEN_URL.to_string())
}

/// Endpoint de uso (`/usages`). Override por `QUOTAL_KIMI_USAGE_URL` solo para tests.
fn usage_url() -> String {
    std::env::var("QUOTAL_KIMI_USAGE_URL").unwrap_or_else(|_| DEFAULT_USAGE_URL.to_string())
}

/// Severidad a partir del porcentaje: >=90 crítico, >=75 aviso, si no normal.
/// Umbral FIJO del contrato (Kimi no devuelve severity propia).
fn severity_of(percent: Option<f64>) -> String {
    match percent {
        Some(p) if p >= 90.0 => "critical".into(),
        Some(p) if p >= 75.0 => "warning".into(),
        _ => "normal".into(),
    }
}

/// Bloque Kimi que viaja al frontend. `configured=false` cuando no hay
/// credenciales de Kimi Code; `available=false` + `error` cuando hay
/// credenciales pero no se pudo obtener el dato (sin red, token irrecuperable…).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KimiPlanInfo {
    pub configured: bool,
    pub available: bool,
    pub error: Option<String>,
    pub membership: Option<String>,
    pub session_percent: Option<f64>,
    pub session_resets_at: Option<String>,
    pub session_severity: String,
    pub weekly_percent: Option<f64>,
    pub weekly_resets_at: Option<String>,
    pub weekly_severity: String,
    /// Marca de tiempo local de la última obtención (o del intento, si no hubo).
    pub fetched_at: Option<String>,
    /// Procedencia del dato: "online" (endpoint en vivo) o "none" (sin credenciales).
    pub source: Option<String>,
}

impl Default for KimiPlanInfo {
    fn default() -> Self {
        KimiPlanInfo {
            configured: false,
            available: false,
            error: None,
            membership: None,
            session_percent: None,
            session_resets_at: None,
            session_severity: "normal".into(),
            weekly_percent: None,
            weekly_resets_at: None,
            weekly_severity: "normal".into(),
            fetched_at: None,
            source: None,
        }
    }
}

impl KimiPlanInfo {
    /// Sin credenciales de Kimi Code: estado honesto "no configurado" (sin error).
    fn unconfigured() -> Self {
        KimiPlanInfo {
            fetched_at: Some(chrono::Local::now().to_rfc3339()),
            source: Some("none".into()),
            ..Default::default()
        }
    }

    /// Con credenciales pero fallo de obtención (red, HTTP, json…).
    fn unavailable(error: impl Into<String>) -> Self {
        KimiPlanInfo {
            configured: true,
            available: false,
            error: Some(error.into()),
            fetched_at: Some(chrono::Local::now().to_rfc3339()),
            ..Default::default()
        }
    }
}

/// Credenciales de Kimi Code CLI: JSON plano (sin envoltorio tipo `claudeAiOauth`).
fn credentials_path() -> PathBuf {
    crate::paths::home().join(".kimi-code").join("credentials").join("kimi-code.json")
}

/// Caché en disco del último dato bueno de Kimi (mismo patrón que plan-cache).
fn cache_path() -> PathBuf {
    crate::paths::widget_dir().join("kimi-cache.json")
}

/// Persiste el último `KimiPlanInfo` correcto (best-effort, atómico).
pub fn save_cache(info: &KimiPlanInfo) {
    if crate::paths::ensure_widget_dir().is_err() {
        return;
    }
    let Ok(json) = serde_json::to_string(info) else {
        return;
    };
    let path = cache_path();
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Carga el último dato cacheado de Kimi (si lo hay y es válido).
pub fn load_cache() -> Option<KimiPlanInfo> {
    let raw = std::fs::read_to_string(cache_path()).ok()?;
    serde_json::from_str::<KimiPlanInfo>(&raw).ok().filter(|k| k.available)
}

struct Creds {
    access_token: String,
    refresh_token: String,
    /// Epoch en SEGUNDOS (ojo: Claude usa milisegundos).
    expires_at_secs: i64,
}

/// Tokens vigentes en memoria. Evita re-refrescar en cada sondeo y preserva el
/// refresh_token ROTADO aunque falle la escritura del fichero.
#[derive(Clone)]
struct Tokens {
    access: String,
    refresh: String,
    expires_at_secs: i64,
}

fn token_cache() -> &'static Mutex<Option<Tokens>> {
    static CACHE: OnceLock<Mutex<Option<Tokens>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn store_cache(t: &Tokens) {
    if let Ok(mut c) = token_cache().lock() {
        *c = Some(t.clone());
    }
}

/// Vacía la caché de tokens en memoria. SOLO para tests (la caché es un `static`
/// compartido por todo el binario de test).
#[cfg(test)]
pub(crate) fn reset_token_cache_for_test() {
    if let Ok(mut c) = token_cache().lock() {
        *c = None;
    }
}

/// Elige los tokens más recientes entre la caché en memoria y el fichero: si
/// Kimi Code CLI refrescó el fichero por su cuenta, su `expires_at` será mayor
/// y lo preferimos; si fuimos nosotros, la caché va por delante.
fn effective_tokens(file: &Creds) -> Tokens {
    let from_file = Tokens {
        access: file.access_token.clone(),
        refresh: file.refresh_token.clone(),
        expires_at_secs: file.expires_at_secs,
    };
    match token_cache().lock().ok().and_then(|c| c.clone()) {
        Some(cached) if cached.expires_at_secs >= from_file.expires_at_secs => cached,
        _ => from_file,
    }
}

/// Parsea el JSON plano de credenciales de Kimi (`kimi-code.json`).
fn parse_creds_blob(raw: &str) -> Option<Creds> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    Some(Creds {
        access_token: v.get("access_token")?.as_str()?.to_string(),
        refresh_token: v.get("refresh_token").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        expires_at_secs: v.get("expires_at").and_then(|x| x.as_i64()).unwrap_or(0),
    })
}

fn read_creds() -> Option<Creds> {
    let raw = std::fs::read_to_string(credentials_path()).ok()?;
    parse_creds_blob(&raw)
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Cliente HTTP REUTILIZABLE (mismo motivo que en `usage_api.rs`: un cliente por
/// sondeo desperdicia pool de conexiones, DNS y TLS).
fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .unwrap_or_default()
    })
}

/// Cerrojo asíncrono que serializa los refresh de token (patrón SINGLE-FLIGHT).
/// Sin él, dos sondeos concurrentes podrían gastar cada uno el mismo refresh_token.
fn refresh_gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Refresca el access token con el refresh token (POST form-url-encoded, como
/// hace Kimi Code CLI). Actualiza SIEMPRE la caché en memoria (incluido el
/// refresh_token rotado) y, best-effort, reescribe el fichero fusionando campos.
async fn refresh_token(client: &reqwest::Client, refresh: &str) -> Result<Tokens, String> {
    if refresh.is_empty() {
        return Err("sin refresh_token".into());
    }

    // SINGLE-FLIGHT: serializa los refresh para que dos llamadas concurrentes
    // (poller + botón "refrescar" de la UI) NO gasten cada una el refresh_token
    // —que es de un solo uso y ROTA—, lo que invalidaría la sesión de Kimi Code.
    let _gate = refresh_gate().lock().await;

    // Ya con el cerrojo: si OTRA llamada refrescó mientras esperábamos, reutiliza
    // su resultado en vez de lanzar otra petición con un token ya consumido.
    if let Some(cached) = token_cache().lock().ok().and_then(|c| c.clone()) {
        if cached.refresh != refresh && cached.expires_at_secs - now_secs() > 60 {
            return Ok(cached);
        }
    }

    let resp = client
        .post(token_url())
        .header("User-Agent", USER_AGENT)
        .form(&[
            ("client_id", KIMI_CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
        ])
        .send()
        .await
        .map_err(|e| format!("red: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("refresh HTTP {}", resp.status().as_u16()));
    }
    let j: serde_json::Value = resp.json().await.map_err(|e| format!("json: {e}"))?;
    let (access, new_refresh, expires_in) = parse_token_response(&j, refresh)?;
    let tokens = Tokens { access, refresh: new_refresh, expires_at_secs: now_secs() + expires_in };

    // La caché es la fuente de verdad para los siguientes sondeos; el fichero es
    // un extra (Kimi Code CLI también lo gestiona). Cachear primero es lo crítico.
    store_cache(&tokens);
    persist_tokens(&tokens.access, &tokens.refresh, tokens.expires_at_secs);
    Ok(tokens)
}

/// Extrae `(access, refresh, expires_in_secs)` de la respuesta JSON del endpoint
/// de token. Si el `refresh_token` no rota, conservamos `prev_refresh`.
/// `expires_in` por defecto 900s (los tokens de Kimi duran 15 min). Función PURA.
fn parse_token_response(
    j: &serde_json::Value,
    prev_refresh: &str,
) -> Result<(String, String, i64), String> {
    let access = j
        .get("access_token")
        .and_then(|x| x.as_str())
        .ok_or("respuesta sin access_token")?
        .to_string();
    let new_refresh =
        j.get("refresh_token").and_then(|x| x.as_str()).unwrap_or(prev_refresh).to_string();
    let expires_in = j.get("expires_in").and_then(|x| x.as_i64()).unwrap_or(900);
    Ok((access, new_refresh, expires_in))
}

/// Aplica los tokens nuevos sobre el blob JSON FUSIONANDO (preserva `scope`,
/// `token_type`, `expires_in` y cualquier campo ajeno). Devuelve el JSON
/// serializado o `None`.
fn merge_tokens_into_blob(
    raw: &str,
    access: &str,
    refresh: &str,
    expires_at_secs: i64,
) -> Option<String> {
    let mut v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let o = v.as_object_mut()?;
    o.insert("access_token".into(), serde_json::Value::String(access.into()));
    o.insert("refresh_token".into(), serde_json::Value::String(refresh.into()));
    o.insert("expires_at".into(), serde_json::Value::Number(expires_at_secs.into()));
    serde_json::to_string(&v).ok()
}

/// Escribe `contents` en `path` con permisos restrictivos (0600 en Unix: el
/// fichero contiene tokens OAuth). Misma política que `usage_api.rs`.
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Reescribe el fichero de credenciales de forma atómica (tmp + rename).
/// Devuelve `true` si lo escribió.
fn persist_tokens_file(access: &str, refresh: &str, expires_at_secs: i64) -> bool {
    let path = credentials_path();
    let Ok(raw) = std::fs::read_to_string(&path) else { return false };

    // COMPARE-AND-SWAP contra Kimi Code CLI: si el fichero ya contiene un token
    // con expiración IGUAL o MÁS NUEVA que el nuestro, él refrescó por su cuenta
    // entre medias → NO lo pisamos.
    if let Some(file_creds) = parse_creds_blob(&raw) {
        if file_creds.expires_at_secs >= expires_at_secs {
            return false;
        }
    }

    let Some(serialized) = merge_tokens_into_blob(&raw, access, refresh, expires_at_secs) else {
        return false;
    };
    let tmp = path.with_extension("json.tmp");
    if write_private(&tmp, &serialized).is_ok() {
        std::fs::rename(&tmp, &path).is_ok()
    } else {
        false
    }
}

/// Persiste los tokens nuevos donde Kimi Code CLI los lee. En modo solo-lectura
/// NO se toca el fichero: el token refrescado sigue en la caché en memoria y el
/// fetch funciona igual, sin efectos secundarios (garantía del modo observador).
fn persist_tokens(access: &str, refresh: &str, expires_at_secs: i64) {
    if crate::app_config::is_read_only() {
        return;
    }
    let _ = persist_tokens_file(access, refresh, expires_at_secs);
}

async fn get_usage(client: &reqwest::Client, token: &str) -> Result<reqwest::Response, String> {
    client
        .get(usage_url())
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("red: {e}"))
}

/// Obtiene el uso real de Kimi. Refresca el token si hace falta. Nunca entra en
/// pánico: sin credenciales devuelve `unconfigured()`, ante cualquier fallo de
/// red devuelve `unavailable(..)`.
pub async fn fetch() -> KimiPlanInfo {
    let Some(creds) = read_creds() else {
        return KimiPlanInfo::unconfigured();
    };

    let client = http_client();

    // Tokens efectivos (caché en memoria o fichero, el más reciente).
    let mut tok = effective_tokens(&creds);

    // Refresca de forma proactiva si caduca en <60s. Con `expires_in: 900` esto
    // es lo habitual: el token de Kimi vive solo 15 minutos.
    if tok.expires_at_secs != 0 && tok.expires_at_secs - now_secs() < 60 {
        if let Ok(t) = refresh_token(client, &tok.refresh).await {
            tok = t;
        }
        // Si falla, seguimos con el token actual; si está caducado, el 401 de
        // abajo dispara un reintento.
    }

    let mut resp = match get_usage(client, &tok.access).await {
        Ok(r) => r,
        Err(e) => return KimiPlanInfo::unavailable(e),
    };

    // Si el token había caducado pese a todo, refrescamos y reintentamos 1 vez.
    if resp.status().as_u16() == 401 {
        match refresh_token(client, &tok.refresh).await {
            Ok(t) => match get_usage(client, &t.access).await {
                Ok(r) => resp = r,
                Err(e) => return KimiPlanInfo::unavailable(e),
            },
            Err(e) => return KimiPlanInfo::unavailable(format!("reautenticación: {e}")),
        }
    }

    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let msg = match code {
            429 => "límite de peticiones (reintentando)".to_string(),
            500..=599 => format!("servidor {code}"),
            _ => format!("HTTP {code}"),
        };
        return KimiPlanInfo::unavailable(msg);
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => return KimiPlanInfo::unavailable(format!("json: {e}")),
    };

    parse_usage(&body)
}

/// Lee un número que la API de Kimi devuelve como STRING ("100") — tolera
/// también números JSON por robustez.
fn str_num(v: Option<&serde_json::Value>) -> Option<f64> {
    match v? {
        serde_json::Value::String(s) => s.trim().parse::<f64>().ok(),
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// Porcentaje used/limit*100 a partir de un detalle {limit, used, resetTime}.
/// Devuelve `(percent, reset)`. Límite 0 o ausente → sin porcentaje. El % se
/// redondea a 1 decimal: 14/100*100 en f64 da 14.000000000000002, un artefacto
/// binario que ensuciaría el payload (el contrato espera 14.0).
fn parse_detail(detail: &serde_json::Value) -> (Option<f64>, Option<String>) {
    let limit = str_num(detail.get("limit"));
    let used = str_num(detail.get("used"));
    let percent = match (used, limit) {
        (Some(u), Some(l)) if l > 0.0 => Some(((u / l * 1000.0).round() / 10.0).clamp(0.0, 100.0)),
        _ => None,
    };
    let reset = detail.get("resetTime").and_then(|x| x.as_str()).map(String::from);
    (percent, reset)
}

/// ¿Es esta entrada de `limits[]` la ventana rodante de 5 HORAS (300 minutos)?
/// `window.duration` puede venir como número (300) o string ("300").
fn is_session_window(lim: &serde_json::Value) -> bool {
    let w = lim.get("window");
    let duration = w.and_then(|w| w.get("duration"));
    let is_300 = match duration {
        Some(serde_json::Value::Number(n)) => n.as_i64() == Some(300),
        Some(serde_json::Value::String(s)) => s.trim() == "300",
        _ => false,
    };
    let is_minutes =
        w.and_then(|w| w.get("timeUnit")).and_then(|x| x.as_str()) == Some("TIME_UNIT_MINUTE");
    is_300 && is_minutes
}

/// Extrae membresía, semana y ventana de 5h del JSON de `/usages`:
///   - `user.membership.level`  -> membresía.
///   - `usage`                  -> cuota SEMANAL (7 días).
///   - `limits[]` (300 min)     -> ventana rodante de 5 HORAS (si no, la primera).
fn parse_usage(body: &serde_json::Value) -> KimiPlanInfo {
    let mut info = KimiPlanInfo {
        configured: true,
        available: true,
        membership: body
            .get("user")
            .and_then(|u| u.get("membership"))
            .and_then(|m| m.get("level"))
            .and_then(|x| x.as_str())
            .map(String::from),
        fetched_at: Some(chrono::Local::now().to_rfc3339()),
        source: Some("online".into()),
        ..Default::default()
    };

    // Cuota semanal (bloque `usage` de nivel raíz).
    if let Some(u) = body.get("usage") {
        let (p, r) = parse_detail(u);
        info.weekly_percent = p;
        info.weekly_resets_at = r;
    }

    // Ventana de 5h: la entrada de `limits[]` con window 300 minutos; si no hay,
    // la primera (defensivo: la API podría añadir/renombrar ventanas).
    if let Some(arr) = body.get("limits").and_then(|x| x.as_array()) {
        let entry = arr.iter().find(|lim| is_session_window(lim)).or_else(|| arr.first());
        if let Some(lim) = entry {
            if let Some(detail) = lim.get("detail") {
                let (p, r) = parse_detail(detail);
                info.session_percent = p;
                info.session_resets_at = r;
            }
        }
    }

    info.session_severity = severity_of(info.session_percent);
    info.weekly_severity = severity_of(info.weekly_percent);
    info
}

// ---------------------------------------------------------------------------
// Tests de parseo (funciones puras; `cargo test`).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_usage_numeros_como_strings() {
        // La API de Kimi devuelve limit/used como STRINGS: hay que parsearlos.
        let body = json!({
            "user": { "membership": { "level": "LEVEL_INTERMEDIATE" } },
            "usage": { "limit": "100", "used": "3", "remaining": "97",
                       "resetTime": "2026-08-03T08:37:38.775091Z" },
            "limits": [
                { "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                  "detail": { "limit": "100", "used": "14", "remaining": "86",
                              "resetTime": "2026-07-27T13:37:38.775091Z" } }
            ]
        });
        let k = parse_usage(&body);
        assert!(k.configured && k.available);
        assert_eq!(k.membership.as_deref(), Some("LEVEL_INTERMEDIATE"));
        assert_eq!(k.session_percent, Some(14.0));
        assert_eq!(k.session_resets_at.as_deref(), Some("2026-07-27T13:37:38.775091Z"));
        assert_eq!(k.weekly_percent, Some(3.0));
        assert_eq!(k.weekly_resets_at.as_deref(), Some("2026-08-03T08:37:38.775091Z"));
        assert_eq!(k.source.as_deref(), Some("online"));
    }

    #[test]
    fn parse_usage_elige_la_ventana_de_300_minutos() {
        // Con varias entradas en `limits[]`, la sesión es la de window=300 MINUTE,
        // NO la primera del array.
        let body = json!({
            "limits": [
                { "window": { "duration": 10080, "timeUnit": "TIME_UNIT_MINUTE" },
                  "detail": { "limit": "100", "used": "3", "resetTime": "w" } },
                { "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                  "detail": { "limit": "100", "used": "14", "resetTime": "s" } }
            ]
        });
        let k = parse_usage(&body);
        assert_eq!(k.session_percent, Some(14.0));
        assert_eq!(k.session_resets_at.as_deref(), Some("s"));
    }

    #[test]
    fn parse_usage_cae_a_la_primera_entrada_si_no_hay_300() {
        // Defensivo: si la API deja de traer la ventana de 300 min, usamos la
        // primera entrada en vez de quedarnos sin sesión.
        let body = json!({
            "limits": [
                { "window": { "duration": 60, "timeUnit": "TIME_UNIT_MINUTE" },
                  "detail": { "limit": "50", "used": "25", "resetTime": "x" } }
            ]
        });
        let k = parse_usage(&body);
        assert_eq!(k.session_percent, Some(50.0));
    }

    #[test]
    fn severity_por_umbral() {
        assert_eq!(severity_of(Some(95.0)), "critical");
        assert_eq!(severity_of(Some(90.0)), "critical");
        assert_eq!(severity_of(Some(80.0)), "warning");
        assert_eq!(severity_of(Some(75.0)), "warning");
        assert_eq!(severity_of(Some(14.0)), "normal");
        assert_eq!(severity_of(None), "normal");
    }

    #[test]
    fn parse_usage_limite_cero_no_porcentaje() {
        // Límite 0 (o no numérico) → percent None y severidad "normal" (no NaN).
        let body = json!({
            "usage": { "limit": "0", "used": "5", "resetTime": "w" },
            "limits": []
        });
        let k = parse_usage(&body);
        assert_eq!(k.weekly_percent, None);
        assert_eq!(k.weekly_severity, "normal");
    }

    #[test]
    fn merge_tokens_preserva_campos_ajenos() {
        // Al reescribir credenciales NO debemos perder `scope`, `token_type`,
        // `expires_in` ni cualquier campo que Kimi Code CLI mantenga.
        let raw = r#"{"access_token":"viejo","refresh_token":"r0","expires_at":1,"expires_in":900,"scope":"s","token_type":"Bearer"}"#;
        let out = merge_tokens_into_blob(raw, "nuevo", "r1", 999).expect("debe serializar");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["access_token"], "nuevo");
        assert_eq!(v["refresh_token"], "r1");
        assert_eq!(v["expires_at"], 999);
        assert_eq!(v["scope"], "s"); // preservado
        assert_eq!(v["token_type"], "Bearer"); // preservado
        assert_eq!(v["expires_in"], 900); // preservado
    }

    #[test]
    fn parse_token_response_rota_refresh_y_default_900() {
        // Rotación de AMBOS tokens (lo habitual en Kimi).
        let j = json!({ "access_token": "a1", "refresh_token": "r1", "expires_in": 900 });
        let (a, r, e) = parse_token_response(&j, "r0").unwrap();
        assert_eq!((a.as_str(), r.as_str(), e), ("a1", "r1", 900));
        // Sin refresh_token en la respuesta: se conserva el previo.
        let j = json!({ "access_token": "a2" });
        let (a, r, e) = parse_token_response(&j, "r0").unwrap();
        assert_eq!((a.as_str(), r.as_str(), e), ("a2", "r0", 900));
        // Sin access_token: error (no cacheamos basura).
        assert!(parse_token_response(&json!({ "refresh_token": "x" }), "p").is_err());
    }

    #[test]
    fn unconfigured_cumple_el_contrato() {
        // Sin credenciales: configured:false, available:false, error:null,
        // severities "normal", source "none", fetched_at presente.
        let k = KimiPlanInfo::unconfigured();
        assert!(!k.configured);
        assert!(!k.available);
        assert!(k.error.is_none());
        assert_eq!(k.session_severity, "normal");
        assert_eq!(k.weekly_severity, "normal");
        assert_eq!(k.source.as_deref(), Some("none"));
        assert!(k.fetched_at.is_some());
        assert!(k.session_percent.is_none() && k.weekly_percent.is_none());
    }
}

// ---------------------------------------------------------------------------
// Tests de INTEGRACIÓN del flujo fetch/refresh contra un servidor HTTP SIMULADO
// (`httpmock`) y un HOME temporal — nunca tocan la red ni el `.kimi-code` real.
// Tocan estado GLOBAL (env vars + caché estática de tokens): van con `#[serial]`.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod net_tests {
    use super::*;
    use httpmock::prelude::*;
    use serde_json::json;
    use serial_test::serial;
    use std::path::Path;

    /// Apunta HOME/USERPROFILE a `dir` y escribe ahí un `kimi-code.json` plano.
    fn setup_home(dir: &Path, access: &str, refresh: &str, expires_at_secs: i64) {
        std::env::set_var("HOME", dir);
        std::env::set_var("USERPROFILE", dir);
        // Parte SIEMPRE de solo-lectura = OFF (una fuga de un test previo no debe
        // bloquear el write-back aquí).
        crate::app_config::set_read_only(false);
        let creds_dir = dir.join(".kimi-code").join("credentials");
        std::fs::create_dir_all(&creds_dir).unwrap();
        let blob = json!({
            "access_token": access,
            "refresh_token": refresh,
            "expires_at": expires_at_secs,
            "expires_in": 900,
            "scope": "s",
            "token_type": "Bearer"
        });
        std::fs::write(creds_dir.join("kimi-code.json"), blob.to_string()).unwrap();
    }

    /// Limpia las env vars que el test inyecta (para no filtrar a otros tests).
    fn teardown() {
        for k in ["QUOTAL_KIMI_USAGE_URL", "QUOTAL_KIMI_TOKEN_URL", "HOME", "USERPROFILE"] {
            std::env::remove_var(k);
        }
        reset_token_cache_for_test();
    }

    fn usages_body() -> serde_json::Value {
        json!({
            "user": { "membership": { "level": "LEVEL_INTERMEDIATE" } },
            "usage": { "limit": "100", "used": "3", "remaining": "97",
                       "resetTime": "2026-08-03T08:37:38.775091Z" },
            "limits": [
                { "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                  "detail": { "limit": "100", "used": "14", "remaining": "86",
                              "resetTime": "2026-07-27T13:37:38.775091Z" } }
            ]
        })
    }

    #[tokio::test]
    #[serial]
    async fn fetch_refresca_y_obtiene_uso_happy_path() {
        // Token caducado (los de Kimi viven 15 min) → refresco proactivo con
        // rotación de AMBOS tokens, GET de uso con el token nuevo y write-back
        // fusionado del fichero de credenciales.
        reset_token_cache_for_test();
        let tmp = tempfile::tempdir().unwrap();
        setup_home(tmp.path(), "acc-expired", "ref-1", now_secs() - 10);

        let server = MockServer::start_async().await;
        let token = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/token")
                    .header("Content-Type", "application/x-www-form-urlencoded")
                    .body_includes("grant_type=refresh_token")
                    .body_includes("refresh_token=ref-1")
                    .body_includes("client_id=17e5f671-d194-4dfb-9706-5516cb48c098");
                then.status(200).json_body(json!({
                    "access_token": "acc-new",
                    "refresh_token": "ref-2",
                    "expires_in": 900,
                    "scope": "s",
                    "token_type": "Bearer"
                }));
            })
            .await;
        let usage = server
            .mock_async(|when, then| {
                when.method(GET).path("/usages").header("Authorization", "Bearer acc-new");
                then.status(200).json_body(usages_body());
            })
            .await;
        std::env::set_var("QUOTAL_KIMI_USAGE_URL", server.url("/usages"));
        std::env::set_var("QUOTAL_KIMI_TOKEN_URL", server.url("/token"));

        let info = fetch().await;

        token.assert_async().await;
        usage.assert_async().await;
        assert!(info.configured);
        assert!(info.available, "debería estar disponible; error={:?}", info.error);
        assert_eq!(info.membership.as_deref(), Some("LEVEL_INTERMEDIATE"));
        assert_eq!(info.session_percent, Some(14.0));
        assert_eq!(info.weekly_percent, Some(3.0));
        assert_eq!(info.source.as_deref(), Some("online"));

        // Write-back FUSIONADO: tokens nuevos + campos ajenos preservados.
        let raw = std::fs::read_to_string(tmp.path().join(".kimi-code/credentials/kimi-code.json"))
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["access_token"], "acc-new", "el access nuevo no se persistió");
        assert_eq!(v["refresh_token"], "ref-2", "el refresh rotado no se persistió");
        assert_eq!(v["scope"], "s", "se perdió un campo ajeno al fusionar");
        assert_eq!(v["token_type"], "Bearer");

        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn fetch_sin_credenciales_es_unconfigured() {
        // HOME sin `.kimi-code` → configured:false, available:false, sin error,
        // SIN tocar la red (no hay mock declarado: cualquier petición fallaría).
        reset_token_cache_for_test();
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("USERPROFILE", tmp.path());

        let info = fetch().await;
        assert!(!info.configured);
        assert!(!info.available);
        assert!(info.error.is_none());
        assert_eq!(info.source.as_deref(), Some("none"));
        assert_eq!(info.session_severity, "normal");
        assert!(info.fetched_at.is_some());

        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn fetch_401_refresca_y_reintenta() {
        // Token aún válido (no hay refresco proactivo) pero el GET responde 401:
        // refrescamos y reintentamos UNA vez.
        reset_token_cache_for_test();
        let tmp = tempfile::tempdir().unwrap();
        setup_home(tmp.path(), "acc-stale", "ref-1", now_secs() + 3600);

        let server = MockServer::start_async().await;
        let u401 = server
            .mock_async(|when, then| {
                when.method(GET).path("/usages").header("Authorization", "Bearer acc-stale");
                then.status(401);
            })
            .await;
        let token = server
            .mock_async(|when, then| {
                when.method(POST).path("/token");
                then.status(200).json_body(json!({
                    "access_token": "acc-new", "refresh_token": "ref-2", "expires_in": 900
                }));
            })
            .await;
        let uok = server
            .mock_async(|when, then| {
                when.method(GET).path("/usages").header("Authorization", "Bearer acc-new");
                then.status(200).json_body(usages_body());
            })
            .await;
        std::env::set_var("QUOTAL_KIMI_USAGE_URL", server.url("/usages"));
        std::env::set_var("QUOTAL_KIMI_TOKEN_URL", server.url("/token"));

        let info = fetch().await;

        u401.assert_async().await;
        token.assert_async().await;
        uok.assert_async().await;
        assert!(info.available, "debería recuperarse tras el 401; error={:?}", info.error);
        assert_eq!(info.session_percent, Some(14.0));

        teardown();
    }

    #[tokio::test]
    #[serial]
    async fn read_only_no_reescribe_credenciales_pero_sigue_funcionando() {
        // GARANTÍA del modo observador: el token se refresca EN MEMORIA y el
        // fetch funciona, pero el fichero `kimi-code.json` NO se toca.
        reset_token_cache_for_test();
        let tmp = tempfile::tempdir().unwrap();
        setup_home(tmp.path(), "acc-expired", "ref-1", now_secs() - 10);
        crate::app_config::set_read_only(true);

        let server = MockServer::start_async().await;
        let _tok = server
            .mock_async(|when, then| {
                when.method(POST).path("/token");
                then.status(200).json_body(json!({
                    "access_token": "acc-new", "refresh_token": "ref-2", "expires_in": 900
                }));
            })
            .await;
        let _uok = server
            .mock_async(|when, then| {
                when.method(GET).path("/usages").header("Authorization", "Bearer acc-new");
                then.status(200).json_body(usages_body());
            })
            .await;
        std::env::set_var("QUOTAL_KIMI_USAGE_URL", server.url("/usages"));
        std::env::set_var("QUOTAL_KIMI_TOKEN_URL", server.url("/token"));

        let info = fetch().await;
        assert!(info.available, "debe funcionar igual; error={:?}", info.error);
        assert_eq!(info.session_percent, Some(14.0));

        // El fichero conserva el token VIEJO: no hubo write-back.
        let raw = std::fs::read_to_string(tmp.path().join(".kimi-code/credentials/kimi-code.json"))
            .unwrap();
        assert!(raw.contains("acc-expired"), "no debió reescribirse el token: {raw}");
        assert!(!raw.contains("acc-new"), "el token nuevo NO debe llegar al disco");

        crate::app_config::set_read_only(false); // limpia el global para otros tests
        teardown();
    }

    #[test]
    #[serial]
    fn persist_tokens_file_no_pisa_si_kimi_cli_ya_refresco() {
        // CAS: el fichero tiene un expiry IGUAL o MÁS NUEVO que el nuestro (Kimi
        // Code CLI refrescó por su cuenta). No debemos pisarlo.
        reset_token_cache_for_test();
        let tmp = tempfile::tempdir().unwrap();
        setup_home(tmp.path(), "cli-fresh", "r-cli", 9000);

        let wrote = persist_tokens_file("nuestro", "r-noso", 5000);
        assert!(!wrote, "no debe pisar un token igual o más nuevo");

        let raw = std::fs::read_to_string(tmp.path().join(".kimi-code/credentials/kimi-code.json"))
            .unwrap();
        assert!(raw.contains("cli-fresh"), "el token de Kimi Code CLI debe quedar intacto");
        assert!(!raw.contains("nuestro"), "nuestro token NO debió escribirse");

        teardown();
    }
}
