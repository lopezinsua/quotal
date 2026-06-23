// usage_api.rs — Datos REALES de límites de plan vía el mismo endpoint que `/usage`.
//
// Flujo:
//   1. Lee el token OAuth local de `~/.claude/.credentials.json` (lo genera y
//      mantiene Claude Code; nosotros lo reutilizamos, nunca lo creamos).
//   2. Si está caducado (o el GET responde 401), lo refresca contra
//      `https://console.anthropic.com/v1/oauth/token` y reescribe el fichero de
//      forma atómica preservando el resto de claves (igual que hace Claude Code).
//   3. GET `https://api.anthropic.com/api/oauth/usage` -> utilización y resets
//      reales de la ventana de sesión (5h) y semanal (7d).
//
// No inventa límites: el % y el reset los calcula el servidor de Anthropic.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const USER_AGENT: &str = "quotal/0.1";

/// Bloque de plan que viaja al frontend. `available=false` + `error` cuando no
/// se pudo obtener (sin credenciales, sin red, token irrecuperable…).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanInfo {
    pub name: String,
    pub available: bool,
    pub error: Option<String>,
    pub session_percent: Option<f64>,
    pub session_resets_at: Option<String>,
    pub session_severity: Option<String>,
    pub weekly_percent: Option<f64>,
    pub weekly_resets_at: Option<String>,
    pub weekly_severity: Option<String>,
    /// Marca de tiempo local de la última obtención correcta.
    pub fetched_at: Option<String>,
    /// Procedencia del dato, para que la UI sea HONESTA sobre si está en vivo:
    ///   "online"     -> respuesta directa del endpoint `/usage` (lo ideal).
    ///   "statusline" -> respaldo OFFLINE leído del JSON del statusLine (sin red).
    /// (`None` en datos no disponibles). Al cargar de caché conserva su origen
    /// original; la antigüedad la delata `fetched_at`.
    pub source: Option<String>,
}

impl PlanInfo {
    fn unavailable(name: &str, error: impl Into<String>) -> Self {
        PlanInfo {
            name: name.to_string(),
            available: false,
            error: Some(error.into()),
            ..Default::default()
        }
    }

    /// Peor severidad entre sesión y semana. Usa el campo `severity` de la API
    /// y, si falta, lo deriva del porcentaje (>= 90 crítico, >=75 aviso).
    pub fn worst_severity(&self) -> &'static str {
        let from_sev = |s: &Option<String>| match s.as_deref() {
            Some("critical") => 2,
            Some("warning") => 1,
            _ => 0,
        };
        let from_pct = |p: Option<f64>| match p {
            Some(p) if p >= 90.0 => 2,
            Some(p) if p >= 75.0 => 1,
            _ => 0,
        };
        let rank = from_sev(&self.session_severity)
            .max(from_sev(&self.weekly_severity))
            .max(from_pct(self.session_percent))
            .max(from_pct(self.weekly_percent));
        match rank {
            2 => "critical",
            1 => "warning",
            _ => "normal",
        }
    }

    /// Severidad de la SESIÓN (su campo `severity` o, si falta, derivada del %).
    /// Es lo que colorea el anillo dinámico de la bandeja (que mide la sesión).
    pub fn session_status(&self) -> &'static str {
        match self.session_severity.as_deref() {
            Some("critical") => return "critical",
            Some("warning") => return "warning",
            _ => {}
        }
        match self.session_percent {
            Some(p) if p >= 90.0 => "critical",
            Some(p) if p >= 75.0 => "warning",
            _ => "normal",
        }
    }

    /// Fracción de la sesión que QUEDA (0..1), o `None` si aún no hay dato.
    pub fn session_remaining(&self) -> Option<f64> {
        self.session_percent.map(|p| ((100.0 - p) / 100.0).clamp(0.0, 1.0))
    }

    /// Texto para el tooltip de la bandeja del sistema.
    pub fn tray_tooltip(&self) -> String {
        if !self.available {
            return format!("Claude {} · sin conexión", self.name);
        }
        let fmt = |p: Option<f64>| p.map(|p| format!("{p:.0}%")).unwrap_or_else(|| "—".into());
        format!(
            "Claude {} · Sesión {} · Semana {}",
            self.name,
            fmt(self.session_percent),
            fmt(self.weekly_percent)
        )
    }
}

fn credentials_path() -> PathBuf {
    crate::paths::claude_dir().join(".credentials.json")
}

/// Caché en disco del último plan bueno. Permite mostrar datos al instante al
/// arrancar (sin esperar al primer sondeo de 60s) y sirve de respaldo si el
/// endpoint devuelve 429 nada más abrir.
fn cache_path() -> PathBuf {
    crate::paths::widget_dir().join("plan-cache.json")
}

/// Persiste el último `PlanInfo` correcto (best-effort, atómico).
pub fn save_cache(info: &PlanInfo) {
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

/// Carga el último plan cacheado (si lo hay y es válido).
pub fn load_cache() -> Option<PlanInfo> {
    let raw = std::fs::read_to_string(cache_path()).ok()?;
    serde_json::from_str::<PlanInfo>(&raw).ok().filter(|p| p.available)
}

struct Creds {
    access_token: String,
    refresh_token: String,
    expires_at_ms: i64,
    subscription_type: String,
}

/// Tokens vigentes en memoria. Evita re-refrescar en cada sondeo y, sobre todo,
/// preserva el refresh_token ROTADO aunque falle la escritura del fichero (de
/// lo contrario invalidaríamos la sesión de Claude Code).
#[derive(Clone)]
struct Tokens {
    access: String,
    refresh: String,
    expires_at_ms: i64,
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

/// Elige los tokens más recientes entre la caché en memoria y el fichero: si
/// Claude Code refrescó el fichero por su cuenta, su `expiresAt` será mayor y lo
/// preferimos; si fuimos nosotros, la caché va por delante.
fn effective_tokens(file: &Creds) -> Tokens {
    let from_file = Tokens {
        access: file.access_token.clone(),
        refresh: file.refresh_token.clone(),
        expires_at_ms: file.expires_at_ms,
    };
    match token_cache().lock().ok().and_then(|c| c.clone()) {
        Some(cached) if cached.expires_at_ms >= from_file.expires_at_ms => cached,
        _ => from_file,
    }
}

/// Parsea el blob JSON de credenciales (la clave `claudeAiOauth`). Mismo formato
/// en las tres plataformas: cambia DÓNDE se guarda, no el contenido.
fn parse_creds_blob(raw: &str) -> Option<Creds> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let o = v.get("claudeAiOauth")?;
    Some(Creds {
        access_token: o.get("accessToken")?.as_str()?.to_string(),
        refresh_token: o.get("refreshToken").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        expires_at_ms: o.get("expiresAt").and_then(|x| x.as_i64()).unwrap_or(0),
        subscription_type: o
            .get("subscriptionType")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Lee las credenciales del fichero `~/.claude/.credentials.json`. Es la vía en
/// Windows y Linux, y también en macOS si Claude Code usó el fallback de fichero.
fn read_creds_file() -> Option<Creds> {
    let raw = std::fs::read_to_string(credentials_path()).ok()?;
    parse_creds_blob(&raw)
}

/// Servicio bajo el que Claude Code guarda el token en el Keychain de macOS.
#[cfg(target_os = "macos")]
const MAC_KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// Lee el blob de credenciales del Keychain de macOS con la utilidad `security`
/// (la misma que usa Claude Code). Devuelve el JSON crudo o `None`.
#[cfg(target_os = "macos")]
fn keychain_blob() -> Option<String> {
    let user = std::env::var("USER").ok()?;
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", MAC_KEYCHAIN_SERVICE, "-a", &user, "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Credenciales multiplataforma: fichero primero (Windows/Linux y fallback de
/// macOS); si no, Keychain en macOS. Mismo formato de blob en todos los casos.
fn read_creds() -> Option<Creds> {
    if let Some(c) = read_creds_file() {
        return Some(c);
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(c) = keychain_blob().as_deref().and_then(parse_creds_blob) {
            return Some(c);
        }
    }
    None
}

/// "pro" -> "Pro", "max" -> "Max", etc. Fallback: capitaliza el organizationType.
fn plan_label(subscription_type: &str) -> String {
    let s = subscription_type.trim().to_lowercase();
    let pretty = match s.as_str() {
        "" => return org_type_label(),
        "pro" => "Pro",
        "max" => "Max",
        "max_5x" | "max5x" => "Max 5×",
        "max_20x" | "max20x" => "Max 20×",
        "team" => "Team",
        "enterprise" => "Enterprise",
        "free" => "Free",
        other => {
            let mut c = other.chars();
            return c
                .next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_else(|| "Plan".to_string());
        }
    };
    pretty.to_string()
}

/// Lee `oauthAccount.organizationType` de `~/.claude.json` como respaldo.
fn org_type_label() -> String {
    let path = crate::paths::home().join(".claude.json");
    let label = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| {
            v.get("oauthAccount")
                .and_then(|o| o.get("organizationType"))
                .and_then(|x| x.as_str())
                .map(String::from)
        });
    match label.as_deref() {
        Some("claude_pro") => "Pro".to_string(),
        Some("claude_max") => "Max".to_string(),
        Some("claude_team") => "Team".to_string(),
        Some("claude_enterprise") => "Enterprise".to_string(),
        Some(other) => other.trim_start_matches("claude_").replace('_', " "),
        None => "Plan".to_string(),
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Cliente HTTP REUTILIZABLE. Construir uno en cada sondeo (cada 60s) desperdicia
/// el pool de conexiones, la resolución DNS y el handshake TLS; lo creamos una
/// sola vez y lo compartimos. Si el builder con timeout fallara (extremadamente
/// raro), caemos a un cliente por defecto en lugar de entrar en pánico.
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

/// Refresca el access token con el refresh token. Actualiza SIEMPRE la caché en
/// memoria (incluido el refresh_token rotado) y, best-effort, reescribe el
/// fichero de credenciales de forma atómica preservando las demás claves.
async fn refresh_token(client: &reqwest::Client, refresh: &str) -> Result<Tokens, String> {
    if refresh.is_empty() {
        return Err("sin refresh_token".into());
    }

    // SINGLE-FLIGHT: serializa los refresh para que dos llamadas concurrentes (p. ej.
    // el bucle periódico del poller + el botón "refrescar" de la UI) NO gasten cada
    // una el refresh_token —que es de un solo uso y ROTA—, lo que invalidaría la
    // sesión OAuth compartida con Claude Code.
    let _gate = refresh_gate().lock().await;

    // Ya con el cerrojo: si OTRA llamada refrescó mientras esperábamos, la caché
    // tendrá un refresh_token DISTINTO al que íbamos a usar y aún vigente → ese
    // refresh ya rotó el token con éxito; reutiliza su resultado en vez de lanzar
    // otra petición (que fallaría al usar un token ya consumido).
    if let Some(cached) = token_cache().lock().ok().and_then(|c| c.clone()) {
        if cached.refresh != refresh && cached.expires_at_ms - now_ms() > 60_000 {
            return Ok(cached);
        }
    }

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh,
        "client_id": OAUTH_CLIENT_ID,
    });
    let resp = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("red: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("refresh HTTP {}", resp.status().as_u16()));
    }
    let j: serde_json::Value = resp.json().await.map_err(|e| format!("json: {e}"))?;
    let (access, new_refresh, expires_in) = parse_token_response(&j, refresh)?;
    let tokens =
        Tokens { access, refresh: new_refresh, expires_at_ms: now_ms() + expires_in * 1000 };

    // La caché es la fuente de verdad para los siguientes sondeos; el fichero es
    // un extra (Claude Code también lo gestiona). Cachear primero es lo crítico.
    store_cache(&tokens);
    persist_tokens(&tokens.access, &tokens.refresh, tokens.expires_at_ms);
    Ok(tokens)
}

/// Extrae `(access, refresh, expires_in_secs)` de la respuesta JSON del endpoint
/// de token. El `refresh_token` puede NO rotar: si la respuesta no lo trae,
/// conservamos `prev_refresh`. `expires_in` por defecto 3600s. Función PURA ->
/// testeable sin red, que es la parte que puede romper la cuenta del usuario.
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
    let expires_in = j.get("expires_in").and_then(|x| x.as_i64()).unwrap_or(3600);
    Ok((access, new_refresh, expires_in))
}

/// Aplica los tokens nuevos sobre un blob JSON preservando el resto de claves
/// (scopes, mcpOAuth, etc.). Devuelve el JSON serializado o `None`.
fn merge_tokens_into_blob(
    raw: &str,
    access: &str,
    refresh: &str,
    expires_at_ms: i64,
) -> Option<String> {
    let mut v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let o = v.get_mut("claudeAiOauth").and_then(|x| x.as_object_mut())?;
    o.insert("accessToken".into(), serde_json::Value::String(access.into()));
    o.insert("refreshToken".into(), serde_json::Value::String(refresh.into()));
    o.insert("expiresAt".into(), serde_json::Value::Number(expires_at_ms.into()));
    serde_json::to_string(&v).ok()
}

/// Escribe `contents` en `path` con permisos restrictivos. En Unix fuerza 0600
/// (solo el dueño lee/escribe): el fichero contiene tokens OAuth y `std::fs::write`
/// los dejaría en 0644 (legibles por otros usuarios locales). En Windows el ACL del
/// perfil de usuario ya protege el fichero, así que basta con la escritura normal.
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
/// Devuelve `true` si lo escribió (el fichero existía y era válido).
fn persist_tokens_file(access: &str, refresh: &str, expires_at_ms: i64) -> bool {
    let path = credentials_path();
    let Ok(raw) = std::fs::read_to_string(&path) else { return false };

    // COMPARE-AND-SWAP contra Claude Code: relemos JUSTO antes de escribir. Si el
    // fichero ya contiene un token con expiración IGUAL o MÁS NUEVA que el nuestro,
    // Claude Code lo refrescó por su cuenta entre medias → NO lo pisamos (el suyo es
    // al menos tan reciente). Esto acota la ventana de carrera a los microsegundos
    // entre esta relectura y el rename atómico. Un cerrojo de fichero advisory NO
    // ayudaría aquí: Claude Code (Node) no tomaría nuestro lock, así que la única
    // defensa real entre procesos es este chequeo de versión.
    if let Some(file_creds) = parse_creds_blob(&raw) {
        if file_creds.expires_at_ms >= expires_at_ms {
            return false;
        }
    }

    let Some(serialized) = merge_tokens_into_blob(&raw, access, refresh, expires_at_ms) else {
        return false;
    };
    let tmp = path.with_extension("json.tmp");
    if write_private(&tmp, &serialized).is_ok() {
        std::fs::rename(&tmp, &path).is_ok()
    } else {
        false
    }
}

/// Reescribe el token en el Keychain de macOS (`security add-generic-password -U`),
/// preservando el resto del blob. Mantiene a Claude Code en sincronía cuando el
/// refresh_token ha ROTADO (si no, su token guardado quedaría inválido).
#[cfg(target_os = "macos")]
fn persist_tokens_keychain(access: &str, refresh: &str, expires_at_ms: i64) {
    let Some(blob) = keychain_blob() else { return };
    // Mismo CAS que en el fichero: no pisar un token del Keychain igual o más nuevo.
    if let Some(kc) = parse_creds_blob(&blob) {
        if kc.expires_at_ms >= expires_at_ms {
            return;
        }
    }
    let Some(serialized) = merge_tokens_into_blob(&blob, access, refresh, expires_at_ms) else {
        return;
    };
    let Ok(user) = std::env::var("USER") else { return };
    let _ = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-U", // actualiza si ya existe
            "-s",
            MAC_KEYCHAIN_SERVICE,
            "-a",
            &user,
            "-w",
            &serialized,
        ])
        .output();
}

/// Persiste los tokens nuevos allí donde Claude Code los lee, según el SO:
/// fichero en Windows/Linux (y macOS con fallback de fichero), Keychain en macOS.
fn persist_tokens(access: &str, refresh: &str, expires_at_ms: i64) {
    // `_wrote`: usado en macOS (para decidir el fallback de Keychain) e ignorado
    // en Win/Linux, donde solo existe el fichero. El prefijo `_` evita el aviso de
    // variable sin usar sin recurrir a un `return` que clippy marca como redundante.
    let _wrote = persist_tokens_file(access, refresh, expires_at_ms);
    #[cfg(target_os = "macos")]
    if !_wrote {
        persist_tokens_keychain(access, refresh, expires_at_ms);
    }
}

async fn get_usage(client: &reqwest::Client, token: &str) -> Result<reqwest::Response, String> {
    client
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", OAUTH_BETA)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("red: {e}"))
}

/// Recuperación de la carrera de rotación del refresh_token: tras un refresh
/// FALLIDO, relee `.credentials.json` por si Claude Code acaba de dejar ahí un
/// access token nuevo y válido (refrescó él antes que nosotros). Reintenta el
/// GET de uso con ese token. Devuelve la respuesta solo si el token del fichero
/// es DISTINTO al que ya probamos (si es el mismo, no aporta nada y no gastamos
/// otra petición). `None` si no hay nada nuevo que probar o el reintento falla.
async fn recover_with_fresh_creds(
    client: &reqwest::Client,
    tried_access: &str,
) -> Option<reqwest::Response> {
    let fresh = read_creds()?;
    if fresh.access_token == tried_access || fresh.access_token.is_empty() {
        return None;
    }
    get_usage(client, &fresh.access_token).await.ok()
}

/// Obtiene el uso real del plan. Refresca el token si hace falta. Nunca entra en
/// pánico: ante cualquier fallo devuelve `PlanInfo::unavailable(..)`.
pub async fn fetch() -> PlanInfo {
    let Some(creds) = read_creds() else {
        // No pudimos leer credenciales: es "sin login" (normal) o DERIVA de
        // esquema (el fichero está pero renombraron `accessToken`)? El detector
        // distingue ambos casos para no dar falsos positivos.
        let detail =
            std::fs::read_to_string(credentials_path()).ok().and_then(|r| creds_drift_from_raw(&r));
        crate::schema_watch::report("credentials", detail);
        return PlanInfo::unavailable("Plan", "sin credenciales de Claude Code");
    };
    crate::schema_watch::report("credentials", None); // parseó bien
    let name = plan_label(&creds.subscription_type);

    let client = http_client();

    // Tokens efectivos (caché en memoria o fichero, el más reciente).
    let mut tok = effective_tokens(&creds);

    // Refresca de forma proactiva si caduca en <60s.
    if tok.expires_at_ms != 0 && tok.expires_at_ms - now_ms() < 60_000 {
        if let Ok(t) = refresh_token(client, &tok.refresh).await {
            tok = t;
        }
        // Si falla, seguimos con el token actual; si está caducado, el 401 de
        // abajo dispara un reintento.
    }

    let mut resp = match get_usage(client, &tok.access).await {
        Ok(r) => r,
        Err(e) => return PlanInfo::unavailable(&name, e),
    };

    // Si el token había caducado pese a todo, refrescamos y reintentamos 1 vez.
    if resp.status().as_u16() == 401 {
        match refresh_token(client, &tok.refresh).await {
            Ok(t) => match get_usage(client, &t.access).await {
                Ok(r) => resp = r,
                Err(e) => return PlanInfo::unavailable(&name, e),
            },
            // El refresh falló. El motivo más común es la CARRERA DE ROTACIÓN: el
            // refresh_token es de un solo uso y Claude Code pudo haberlo rotado
            // por su cuenta, dejando inválido el que teníamos. En ese caso Claude
            // Code ya habrá reescrito `.credentials.json` con un access token
            // NUEVO y válido: releemos el fichero y reintentamos una vez con él
            // antes de rendirnos (así un refresh ajeno no nos deja "sin conexión").
            Err(e) => match recover_with_fresh_creds(client, &tok.access).await {
                Some(r) => {
                    resp = r;
                    // Adopta los tokens recién leídos del fichero como verdad en
                    // memoria, para no repetir el refresh fallido en el próximo
                    // sondeo.
                    if let Some(fresh) = read_creds() {
                        let t = effective_tokens(&fresh);
                        store_cache(&t);
                    }
                }
                None => return PlanInfo::unavailable(&name, format!("reautenticación: {e}")),
            },
        }
    }

    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        let msg = match code {
            429 => "límite de peticiones (reintentando)".to_string(),
            500..=599 => format!("servidor {code}"),
            _ => format!("HTTP {code}"),
        };
        return PlanInfo::unavailable(&name, msg);
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => return PlanInfo::unavailable(&name, format!("json: {e}")),
    };

    let info = parse_usage(&name, &body);
    // Un 200 del que no extraemos NADA reconocible es señal fuerte de cambio de
    // formato del endpoint (no de "sin datos": un 200 siempre trae los límites).
    crate::schema_watch::report("usage_api", usage_drift(&info));
    info
}

/// Detector PURO de deriva en el blob de credenciales: si el fichero es JSON con
/// la clave `claudeAiOauth` pero SIN `accessToken`, el formato cambió. Sin esa
/// clave (o JSON inválido) → `None`: es "sin login", no deriva.
fn creds_drift_from_raw(raw: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let o = v.get("claudeAiOauth")?;
    if o.get("accessToken").and_then(|x| x.as_str()).is_some() {
        None
    } else {
        Some("`claudeAiOauth` presente pero sin `accessToken`".into())
    }
}

/// Detector PURO de deriva del endpoint `/usage`: un 200 sin sesión NI semana
/// utilizables indica que cambiaron los nombres de los bloques.
fn usage_drift(info: &PlanInfo) -> Option<String> {
    if info.session_percent.is_some() || info.weekly_percent.is_some() {
        None
    } else {
        Some("respuesta 200 sin five_hour/seven_day/limits utilizables".into())
    }
}

/// Respaldo OFFLINE del plan: lee los `rate_limits` OFICIALES del JSON que el
/// statusLine de Claude Code vuelca en el archivo de captura. Devuelve `None`
/// si no hay captura o no trae rate_limits (cuenta sin suscripción, o sesión
/// aún sin primera respuesta). Solo aplica si el puente statusLine está activo.
pub fn plan_from_statusline() -> Option<PlanInfo> {
    let raw = std::fs::read_to_string(crate::paths::capture_path()).ok()?;
    let value: serde_json::Value = raw
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .or_else(|| serde_json::from_str(&raw).ok())?;
    let rl = value.get("rate_limits")?;

    let to_iso = |secs: i64| chrono::DateTime::from_timestamp(secs, 0).map(|d| d.to_rfc3339());
    let read = |key: &str| -> (Option<f64>, Option<String>) {
        let b = rl.get(key);
        let pct = b.and_then(|b| b.get("used_percentage")).and_then(|x| x.as_f64());
        let reset = b.and_then(|b| b.get("resets_at")).and_then(|x| x.as_i64()).and_then(to_iso);
        (pct, reset)
    };
    let (sp, sr) = read("five_hour");
    let (wp, wr) = read("seven_day");
    if sp.is_none() && wp.is_none() {
        return None;
    }

    let name = read_creds()
        .map(|c| plan_label(&c.subscription_type))
        .unwrap_or_else(|| "Plan".to_string());

    Some(PlanInfo {
        name,
        available: true,
        error: None,
        session_percent: sp,
        session_resets_at: sr,
        session_severity: None,
        weekly_percent: wp,
        weekly_resets_at: wr,
        weekly_severity: None,
        fetched_at: Some(chrono::Local::now().to_rfc3339()),
        source: Some("statusline".into()),
    })
}

/// Extrae sesión/semana del JSON. Prefiere el array `limits[]` (trae severity);
/// cae a `five_hour` / `seven_day` si no está.
fn parse_usage(name: &str, body: &serde_json::Value) -> PlanInfo {
    let mut info = PlanInfo {
        name: name.to_string(),
        available: true,
        fetched_at: Some(chrono::Local::now().to_rfc3339()),
        source: Some("online".into()),
        ..Default::default()
    };

    // Bloques directos.
    if let Some(b) = body.get("five_hour") {
        info.session_percent = b.get("utilization").and_then(|x| x.as_f64());
        info.session_resets_at = b.get("resets_at").and_then(|x| x.as_str()).map(String::from);
    }
    if let Some(b) = body.get("seven_day") {
        info.weekly_percent = b.get("utilization").and_then(|x| x.as_f64());
        info.weekly_resets_at = b.get("resets_at").and_then(|x| x.as_str()).map(String::from);
    }

    // Array `limits[]`: aporta severity y, si faltara, percent/reset por grupo.
    if let Some(arr) = body.get("limits").and_then(|x| x.as_array()) {
        for lim in arr {
            let group = lim.get("group").and_then(|x| x.as_str()).unwrap_or("");
            let pct = lim.get("percent").and_then(|x| x.as_f64());
            let reset = lim.get("resets_at").and_then(|x| x.as_str()).map(String::from);
            let sev = lim.get("severity").and_then(|x| x.as_str()).map(String::from);
            match group {
                "session" => {
                    if info.session_percent.is_none() {
                        info.session_percent = pct;
                    }
                    if info.session_resets_at.is_none() {
                        info.session_resets_at = reset;
                    }
                    info.session_severity = sev;
                }
                "weekly" => {
                    if info.weekly_percent.is_none() {
                        info.weekly_percent = pct;
                    }
                    if info.weekly_resets_at.is_none() {
                        info.weekly_resets_at = reset;
                    }
                    info.weekly_severity = sev;
                }
                _ => {}
            }
        }
    }

    info
}

// ---------------------------------------------------------------------------
// Tests de parseo del plan (funciones puras; `cargo test`).
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_usage_bloques_directos() {
        // El servidor expone la utilización como `five_hour` (sesión 5h) y
        // `seven_day` (semana 7d).
        let body = json!({
            "five_hour": { "utilization": 42.0, "resets_at": "2026-06-21T10:00:00Z" },
            "seven_day": { "utilization": 34.0, "resets_at": "2026-06-25T10:00:00Z" }
        });
        let p = parse_usage("Pro", &body);
        assert!(p.available);
        assert_eq!(p.session_percent, Some(42.0));
        assert_eq!(p.weekly_percent, Some(34.0));
        assert_eq!(p.source.as_deref(), Some("online"));
    }

    #[test]
    fn parse_usage_array_limits_aporta_severidad() {
        let body = json!({
            "limits": [
                { "group": "session", "percent": 92.0, "severity": "critical", "resets_at": "x" },
                { "group": "weekly",  "percent": 50.0, "severity": "normal",   "resets_at": "y" }
            ]
        });
        let p = parse_usage("Max", &body);
        assert_eq!(p.session_percent, Some(92.0));
        assert_eq!(p.session_severity.as_deref(), Some("critical"));
        assert_eq!(p.worst_severity(), "critical");
    }

    #[test]
    fn worst_severity_se_deriva_del_porcentaje_si_no_hay_severity() {
        // Sin campo `severity`, ≥75% = aviso, ≥90% = crítico.
        let p = PlanInfo { session_percent: Some(80.0), ..Default::default() };
        assert_eq!(p.worst_severity(), "warning");
    }

    #[test]
    fn merge_tokens_preserva_claves_ajenas() {
        // Al reescribir credenciales NO debemos perder otras claves que Claude Code
        // mantiene (scopes, mcpOAuth…) ni nada fuera de `claudeAiOauth`.
        let raw = r#"{"claudeAiOauth":{"accessToken":"viejo","refreshToken":"r0","expiresAt":1,"scopes":["a","b"]},"otra":42}"#;
        let out = merge_tokens_into_blob(raw, "nuevo", "r1", 999).expect("debe serializar");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["claudeAiOauth"]["accessToken"], "nuevo");
        assert_eq!(v["claudeAiOauth"]["refreshToken"], "r1");
        assert_eq!(v["claudeAiOauth"]["expiresAt"], 999);
        // Claves preservadas:
        assert_eq!(v["claudeAiOauth"]["scopes"][1], "b");
        assert_eq!(v["otra"], 42);
    }

    #[test]
    fn merge_tokens_rechaza_blob_sin_claudeaioauth() {
        // Si el fichero no tiene la forma esperada, devolvemos None (no escribimos).
        assert!(merge_tokens_into_blob(r#"{"x":1}"#, "a", "b", 1).is_none());
        assert!(merge_tokens_into_blob("no es json", "a", "b", 1).is_none());
    }

    #[test]
    fn parse_token_response_con_refresh_rotado() {
        // El servidor devuelve un refresh_token NUEVO (rotación): lo adoptamos.
        let j = json!({ "access_token": "acc1", "refresh_token": "ref_nuevo", "expires_in": 7200 });
        let (a, r, e) = parse_token_response(&j, "ref_viejo").unwrap();
        assert_eq!(a, "acc1");
        assert_eq!(r, "ref_nuevo");
        assert_eq!(e, 7200);
    }

    #[test]
    fn parse_token_response_sin_refresh_conserva_el_anterior() {
        // El servidor NO devuelve refresh_token: hay que conservar el previo (si no,
        // perderíamos la capacidad de refrescar y romperíamos la sesión).
        let j = json!({ "access_token": "acc2" });
        let (a, r, e) = parse_token_response(&j, "ref_previo").unwrap();
        assert_eq!(a, "acc2");
        assert_eq!(r, "ref_previo"); // conservado
        assert_eq!(e, 3600); // expires_in por defecto
    }

    #[test]
    fn parse_token_response_sin_access_token_es_error() {
        // Sin access_token la respuesta es inservible -> Err (no cacheamos basura).
        let j = json!({ "refresh_token": "x", "expires_in": 100 });
        assert!(parse_token_response(&j, "prev").is_err());
    }

    #[test]
    fn creds_drift_distingue_sin_login_de_cambio_de_formato() {
        // Token presente -> sin deriva.
        let ok = r#"{"claudeAiOauth":{"accessToken":"a","refreshToken":"r"}}"#;
        assert!(creds_drift_from_raw(ok).is_none());
        // `claudeAiOauth` presente pero SIN accessToken -> DERIVA.
        let drift = r#"{"claudeAiOauth":{"refreshToken":"r"}}"#;
        assert!(creds_drift_from_raw(drift).is_some());
        // Sin la clave (o JSON inválido) -> NO es deriva (sin login).
        assert!(creds_drift_from_raw(r#"{"otra":1}"#).is_none());
        assert!(creds_drift_from_raw("no json").is_none());
    }

    #[test]
    fn usage_drift_solo_marca_si_no_hay_nada_utilizable() {
        let vacio = PlanInfo::default();
        assert!(usage_drift(&vacio).is_some()); // 200 sin nada reconocible -> deriva
        let con_sesion = PlanInfo { session_percent: Some(10.0), ..Default::default() };
        assert!(usage_drift(&con_sesion).is_none());
    }
}
