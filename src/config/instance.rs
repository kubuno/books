//! Instance-wide settings of the books module, as the administrator left them
//! in the console.
//!
//! Declared by `module.toml`'s `[[settings]]`, stored in `core.settings`, and
//! read back here through `/internal/modules/books/settings` — a module owns its
//! own schema and cannot read the core's tables, and a background worker has no
//! user token for the public config route.
//!
//! ## Why these live here and not in `books.settings`
//!
//! The module already has a key/value table (`books.settings`) holding
//! `metadata_language`, edited through a hand-written admin view. That is the
//! right home for a value the console has to render specially. It is the wrong
//! home for a plain switch: every read would be a query, every new key would
//! need a field added to a closed request body, and none of it would show up in
//! the console's search, inheritance or audit trail. Scalars belong to the
//! manifest; `books.settings` keeps what needs a view of its own.
//!
//! Every field here is read by code that acts on it: a knob that changes
//! nothing is worse than an absent one.

use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub struct InstanceConfig {
    /// Whether a reader may pull the original file out of the library. Reading
    /// in the browser is unaffected — the page images keep being served.
    pub allow_downloads: bool,
    /// Whether the OPDS catalogue is served at all. It is an unbrowsable,
    /// app-facing surface that hands out acquisition links, so an instance that
    /// does not use it should be able to close it rather than trust that nobody
    /// found the URL.
    pub opds_enabled: bool,
    /// Whether content carrying NO age rating is withheld from readers who have
    /// an age ceiling. Off, an unrated book is readable by everybody — which is
    /// the whole library until somebody rates it.
    pub block_unrated: bool,
    /// Whether the scanner reads `<AgeRating>` out of ComicInfo.xml. On by
    /// default: without it, `age_rating` stays empty and the age ceiling has
    /// nothing to compare against.
    pub import_age_rating: bool,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            allow_downloads:   true,
            opds_enabled:      true,
            block_unrated:     false,
            import_age_rating: true,
        }
    }
}

impl InstanceConfig {
    /// Maps the core's `{key: value}` object onto the struct. Every read falls
    /// back to the compiled default rather than to a permissive value.
    pub fn from_settings(settings: &Value) -> Self {
        let d = Self::default();
        let bool_of = |key: &str, fallback: bool| {
            settings.get(key).and_then(Value::as_bool).unwrap_or(fallback)
        };
        Self {
            allow_downloads:   bool_of("allow_downloads", d.allow_downloads),
            opds_enabled:      bool_of("opds_enabled", d.opds_enabled),
            block_unrated:     bool_of("block_unrated", d.block_unrated),
            import_age_rating: bool_of("import_age_rating", d.import_age_rating),
        }
    }

    /// The literal spliced into the `books.content_ok(...)` calls of every
    /// listing query.
    ///
    /// Splicing rather than binding is deliberate: the value is the module's own
    /// setting, never anything a request carries, and the queries of this module
    /// are assembled from `&'static str` fragments exactly this way (see
    /// `handlers::content::VISIBLE`). Binding it would mean renumbering every
    /// positional parameter of a dozen queries, which is how a filter ends up
    /// silently applied to the wrong column.
    pub fn block_unrated_sql(&self) -> &'static str {
        if self.block_unrated { "TRUE" } else { "FALSE" }
    }
}

/// Reads the instance settings from the core. Any failure yields `None`, so the
/// caller keeps the values it already had rather than reverting to defaults
/// because the core was briefly unreachable.
pub async fn fetch(http: &reqwest::Client, core_url: &str, secret: &str) -> Option<InstanceConfig> {
    let url = format!("{core_url}/internal/modules/books/settings");
    let resp = http
        .get(&url)
        .header("X-Internal-Secret", secret)
        .send()
        .await
        .map_err(|e| tracing::warn!(error = %e, "Lecture des réglages d'instance books"))
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "Réglages d'instance books refusés par le core");
        return None;
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| tracing::warn!(error = %e, "Réglages d'instance books : réponse illisible"))
        .ok()?;

    Some(InstanceConfig::from_settings(body.get("settings")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_keys_keep_the_compiled_defaults() {
        let c = InstanceConfig::from_settings(&json!({}));
        assert!(c.allow_downloads);
        assert!(c.opds_enabled);
        assert!(!c.block_unrated);
        assert!(c.import_age_rating);
    }

    #[test]
    fn a_switch_that_is_turned_off_is_read_as_off() {
        let c = InstanceConfig::from_settings(&json!({
            "allow_downloads": false, "opds_enabled": false, "block_unrated": true,
        }));
        assert!(!c.allow_downloads);
        assert!(!c.opds_enabled);
        assert!(c.block_unrated);
    }

    /// The fragment goes into SQL, so it may only ever be one of two literals —
    /// never anything derived from a value that could travel from a request.
    #[test]
    fn block_unrated_sql_is_a_closed_pair_of_literals() {
        let d = InstanceConfig::default();
        assert_eq!(d.block_unrated_sql(), "FALSE");
        let c = InstanceConfig::from_settings(&json!({ "block_unrated": true }));
        assert_eq!(c.block_unrated_sql(), "TRUE");
    }

    /// A non-boolean value is a mistake, not an instruction to open something.
    #[test]
    fn a_junk_value_never_relaxes_a_protection() {
        let c = InstanceConfig::from_settings(&json!({ "block_unrated": "oui" }));
        assert!(!c.block_unrated);
        let c = InstanceConfig::from_settings(&json!({ "allow_downloads": "non" }));
        assert!(c.allow_downloads); // the compiled default, not a coerced false
    }
}
