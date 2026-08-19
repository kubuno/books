use anyhow::{Context, Result};
use clap::Parser;
use kubuno_books::{config::Settings, router, state::AppState};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use std::time::Duration;

// ── module.toml reader ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Manifest {
    module:        ManifestModule,
    #[serde(default)]
    sidebar_items: Vec<SidebarItemRaw>,
    events:        Option<ManifestEvents>,
    /// Declarative instance settings (e.g. metadata language).
    #[serde(default)]
    settings:      Vec<SettingDefRaw>,
    /// Pages the admin panel is split into (`[[setting_groups]]`).
    #[serde(default)]
    setting_groups: Vec<SettingGroupRaw>,
}

/// One `[[setting_groups]]` entry of module.toml, forwarded verbatim. `id` is a
/// STABLE, UNTRANSLATED slug: it travels in the URL of the admin page.
#[derive(Deserialize, Serialize)]
struct SettingGroupRaw {
    id:          String,
    label:       String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon:        Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    position:    Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

/// One `[[settings]]` entry from module.toml, forwarded verbatim.
#[derive(Deserialize, Serialize)]
struct SettingDefRaw {
    key:         String,
    scope:       String,
    #[serde(rename = "type")]
    value_type:  String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    values:      Option<serde_json::Value>,
    default:     serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label:       Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category:    Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group:       Option<String>,
    #[serde(default)]
    public:      bool,
}

#[derive(Deserialize)]
struct ManifestModule {
    #[allow(dead_code)]
    id:            String,
    display_name:  String,
    description:   Option<String>,
    settings_path: Option<String>,
}

#[derive(Deserialize)]
struct SidebarItemRaw {
    id:       String,
    label:    String,
    icon:     String,
    path:     String,
    position: i32,
}

#[derive(Deserialize)]
struct ManifestEvents {
    #[serde(default)]
    subscribed: Vec<String>,
}

fn load_manifest() -> Option<Manifest> {
    let path = if let Ok(dir) = std::env::var("KUBUNO_MODULE_DIR") {
        std::path::PathBuf::from(dir).join("module.toml")
    } else {
        std::env::current_exe().ok()?.parent()?.join("module.toml")
    };

    let content = std::fs::read_to_string(&path)
        .map_err(|e| tracing::warn!(path = %path.display(), error = %e, "module.toml introuvable"))
        .ok()?;

    toml::from_str::<Manifest>(&content)
        .map_err(|e| tracing::error!(path = %path.display(), error = %e, "module.toml invalide"))
        .ok()
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "kubuno-books", version, about = "Module livres Kubuno")]
struct Cli {
    #[arg(short, long, env = "KB_CONFIG_FILE")]
    config: Option<String>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let _cli = Cli::parse();

    let settings = Settings::load().context("Chargement de la configuration")?;

    let log_level = settings.logging.level.clone();
    let subscriber = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&log_level)),
    );

    match settings.logging.format {
        kubuno_books::config::LogFormat::Json   => subscriber.json().init(),
        kubuno_books::config::LogFormat::Pretty => subscriber.init(),
    }

    tracing::info!("Kubuno Books v{} démarrage…", env!("CARGO_PKG_VERSION"));

    // Security: forbid any process execution on the host (see kubuno-seccomp).
    kubuno_seccomp::lock_down_process_execution("books");

    // PostgreSQL pool — search_path=books,public
    let opts = settings.database.connect_options()?
        .options([("search_path", "books,public")]);
    let pool = PgPoolOptions::new()
        .max_connections(settings.database.max_connections)
        .min_connections(settings.database.min_connections)
        .acquire_timeout(settings.database.connect_timeout)
        .connect_with(opts)
        .await
        .context("Connexion PostgreSQL")?;

    // Migrations
    if settings.database.run_migrations {
        sqlx::query("CREATE SCHEMA IF NOT EXISTS books")
            .execute(&pool)
            .await
            .context("Création du schéma books")?;

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS books._sqlx_migrations (
                version        BIGINT      PRIMARY KEY,
                description    TEXT        NOT NULL,
                installed_on   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                success        BOOLEAN     NOT NULL,
                checksum       BYTEA       NOT NULL,
                execution_time BIGINT      NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .context("Création table books._sqlx_migrations")?;

        let migration_opts = settings.database.connect_options()?
            .options([("search_path", "books,public")]);
        let migration_pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(settings.database.connect_timeout)
            .connect_with(migration_opts)
            .await
            .context("Pool migration books")?;

        sqlx::migrate!("./migrations")
            .run(&migration_pool)
            .await
            .context("Migrations")?;
    }

    let http = Client::new();

    let storage = kubuno_storage::LocalStorage::new(&settings.storage.local_path)
        .await
        .context("Initialisation du stockage local")?;

    // Instance settings: compiled defaults, then one read from the core so the
    // first request already sees the administrator's access policy rather than
    // a permissive minute.
    let instance = Arc::new(std::sync::RwLock::new(
        kubuno_books::config::instance::InstanceConfig::default(),
    ));
    if let Some(cfg) = kubuno_books::config::instance::fetch(
        &http, &settings.core.url, &settings.core.internal_secret,
    ).await {
        if let Ok(mut w) = instance.write() { *w = cfg; }
    }

    let state = AppState {
        db:       pool,
        settings: Arc::new(settings.clone()),
        storage:  Arc::new(storage),
        http:     http.clone(),
        instance: instance.clone(),
    };

    // Instance-settings refresher: an admin edit takes effect within a minute,
    // no restart. A failed read keeps the last good values.
    {
        let http_r     = http.clone();
        let settings_r = settings.clone();
        let instance_r = instance.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                if let Some(cfg) = kubuno_books::config::instance::fetch(
                    &http_r, &settings_r.core.url, &settings_r.core.internal_secret,
                ).await {
                    if let Ok(mut w) = instance_r.write() { *w = cfg; }
                }
            }
        });
    }

    // Register with the core (infinite retry).
    register_with_core(&http, &settings).await;

    // Heartbeat every 30s.
    {
        let http2     = http.clone();
        let settings2 = settings.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let url    = format!("{}/internal/modules/books/heartbeat", settings2.core.url);
                let secret = &settings2.core.internal_secret;
                match http2.post(&url).header("X-Internal-Secret", secret.as_str()).send().await {
                    Ok(r) if r.status().is_success() => {}
                    Ok(r) if r.status() == reqwest::StatusCode::NOT_FOUND => {
                        tracing::info!("Heartbeat 404 — ré-enregistrement…");
                        register_with_core(&http2, &settings2).await;
                    }
                    Ok(r) if r.status() == reqwest::StatusCode::FORBIDDEN => {
                        tracing::info!("Heartbeat 403 — module désactivé, attente…");
                    }
                    Ok(r)  => tracing::warn!(status = %r.status(), "Heartbeat réponse inattendue"),
                    Err(e) => tracing::warn!(error = %e, "Heartbeat erreur réseau"),
                }
            }
        });
    }

    // HTTP server
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Bind sur {addr}"))?;

    tracing::info!("Kubuno Books démarré sur http://{addr}");

    // Background scan scheduler (scan-on-startup + per-library interval).
    kubuno_books::workers::scheduler::spawn(state.clone());

    let app = router::build(state);
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await
        .context("Erreur du serveur HTTP")?;

    Ok(())
}

fn backoff(attempt: u32) -> u64 {
    if attempt <= 10 { (attempt * 2) as u64 } else { 30 }
}

async fn register_with_core(http: &Client, settings: &Settings) {
    let base_url = format!("http://{}:{}", settings.server.host, settings.server.port);
    let core_url = &settings.core.url;
    let secret   = &settings.core.internal_secret;

    let manifest = load_manifest();
    let display_name  = manifest.as_ref().map(|m| m.module.display_name.as_str()).unwrap_or("Books").to_string();
    let description   = manifest.as_ref().and_then(|m| m.module.description.clone());
    let settings_path = manifest.as_ref()
        .and_then(|m| m.module.settings_path.clone())
        .or_else(|| Some("/books/settings".into()));
    let sidebar_items: Vec<Value> = manifest.as_ref()
        .map(|m| m.sidebar_items.iter().map(|s| json!({
            "id":       s.id,
            "label":    s.label,
            "icon":     s.icon,
            "path":     s.path,
            "position": s.position,
        })).collect())
        .unwrap_or_else(|| vec![
            json!({ "id": "books", "label": "Books", "icon": "BookOpen", "path": "/books", "position": 22 }),
        ]);
    let subscribed_events: Vec<String> = manifest.as_ref()
        .and_then(|m| m.events.as_ref())
        .map(|e| e.subscribed.clone())
        .unwrap_or_else(|| vec!["UserDeleted".into()]);

    // Declarative instance settings + admin pages, forwarded so the core can render
    // the generic form and split the admin panel into sub-menus.
    let settings_schema: Vec<Value> = manifest.as_ref()
        .map(|m| m.settings.iter().map(|s| serde_json::to_value(s).unwrap_or(Value::Null)).collect())
        .unwrap_or_default();
    let setting_groups: Vec<Value> = manifest.as_ref()
        .map(|m| m.setting_groups.iter().map(|g| serde_json::to_value(g).unwrap_or(Value::Null)).collect())
        .unwrap_or_default();

    let payload = json!({
        "module_id":         "books",
        "display_name":      display_name,
        "description":       description,
        "settings_path":     settings_path,
        "base_url":          base_url,
        "version":           env!("CARGO_PKG_VERSION"),
        "routes":            [{ "method": "*", "path": "/*" }],
        "sidebar_items":     sidebar_items,
        "subscribed_events": subscribed_events,
        "settings_schema":   settings_schema,
        "setting_groups":    setting_groups,
    });

    for attempt in 1u32.. {
        let url = format!("{core_url}/internal/modules/register");
        match http.post(&url)
            .header("X-Internal-Secret", secret.as_str())
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!("Module books enregistré auprès du core");
                return;
            }
            Ok(resp) if resp.status() == reqwest::StatusCode::FORBIDDEN => {
                tracing::info!(attempt, "Module désactivé par l'admin, nouvel essai dans 30s…");
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }
            Ok(resp) => {
                let wait = backoff(attempt);
                tracing::warn!(attempt, status = %resp.status(), "Enregistrement échoué, retry dans {wait}s…");
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
            Err(e) => {
                let wait = backoff(attempt);
                tracing::warn!(attempt, error = %e, "Core inaccessible, retry dans {wait}s…");
                tokio::time::sleep(Duration::from_secs(wait)).await;
            }
        }
    }
    unreachable!()
}
