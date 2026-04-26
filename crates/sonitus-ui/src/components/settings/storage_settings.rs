//! Storage settings — cache size limit, current cache usage, clear cache,
//! download location, database backup. All preferences persist via
//! `SettingsState`.

use crate::state::settings_state::SettingsState;
use dioxus::prelude::*;
use sonitus_core::config::AppConfig;

/// Storage / cache settings page.
#[component]
pub fn StorageSettings() -> Element {
    let mut settings = use_context::<Signal<SettingsState>>();
    let mut status = use_signal(|| Option::<String>::None);
    let mut error = use_signal(|| Option::<String>::None);

    // Cache used: walk the cache directory once on mount and on every
    // explicit "Clear cache" so the value reflects reality.
    let mut cache_revalidate = use_signal(|| 0u32);
    let cache_used = use_resource(move || {
        let _v = *cache_revalidate.read();
        async move {
            tokio::task::spawn_blocking(|| match AppConfig::cache_dir() {
                Ok(dir) => walk_total_bytes(&dir),
                Err(_) => 0u64,
            })
            .await
            .unwrap_or(0)
        }
    });

    let snap = settings.read().clone();
    let cache_max_gb = (snap.config.cache_max_mb / 1024).max(1);
    let download_location = snap
        .config
        .download_location
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    let on_cache_max = move |evt: FormEvent| {
        let n: u64 = evt.value().parse().unwrap_or(10);
        let mb = n.saturating_mul(1024);
        settings.write().set_cache_max_mb(mb);
    };

    let on_clear_cache = move |_| {
        match AppConfig::cache_dir() {
            Ok(dir) => match clear_dir(&dir) {
                Ok(_) => {
                    status.set(Some("Cache cleared.".into()));
                    error.set(None);
                    let cur = *cache_revalidate.read();
                    cache_revalidate.set(cur.wrapping_add(1));
                }
                Err(e) => error.set(Some(format!("Couldn't clear cache: {e}"))),
            },
            Err(e) => error.set(Some(format!("Couldn't locate cache: {e}"))),
        }
    };

    let on_download_location = move |evt: FormEvent| {
        let v = evt.value();
        let trimmed = v.trim();
        let path = if trimmed.is_empty() {
            None
        } else {
            Some(std::path::PathBuf::from(trimmed))
        };
        settings.write().set_download_location(path);
    };

    let on_pick_download_dir = move |_| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(picked) = rfd::FileDialog::new().pick_folder() {
                settings.write().set_download_location(Some(picked));
            }
        }
    };

    let on_backup_db = move |_| {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Ok(src) = AppConfig::db_path() else {
                error.set(Some("Couldn't locate database.".into()));
                return;
            };
            if !src.exists() {
                error.set(Some("Database file doesn't exist yet.".into()));
                return;
            }
            let Some(dest) = rfd::FileDialog::new()
                .set_file_name("sonitus-library.db")
                .add_filter("SQLite database", &["db"])
                .save_file()
            else {
                return;
            };
            match std::fs::copy(&src, &dest) {
                Ok(_) => {
                    status.set(Some(format!("Backed up to {}", dest.display())));
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("Backup failed: {e}"))),
            }
        }
    };

    let used_bytes = cache_used.read_unchecked().unwrap_or(0);
    let used_label = format_bytes(used_bytes);
    let snap_status = status.read().clone();
    let snap_error = error.read().clone();

    rsx! {
        section { class: "settings-page",
            h1 { "Storage" }
            label { class: "field",
                span { class: "field__label", "Cache size limit (GB)" }
                input {
                    r#type: "number",
                    min: "1",
                    max: "1000",
                    value: "{cache_max_gb}",
                    class: "input",
                    oninput: on_cache_max,
                }
            }
            div { class: "field",
                span { class: "field__label", "Cache used" }
                p { class: "field__value", "{used_label}" }
            }
            div { class: "field",
                button { class: "btn btn--danger", onclick: on_clear_cache, "Clear cache" }
            }
            div { class: "field",
                span { class: "field__label", "Download location" }
                div { class: "field__path-row",
                    input {
                        r#type: "text",
                        placeholder: "(platform default)",
                        class: "input field__path-input",
                        value: "{download_location}",
                        oninput: on_download_location,
                    }
                    button { class: "btn btn--ghost", onclick: on_pick_download_dir, "Browse…" }
                }
            }
            div { class: "field",
                span { class: "field__label", "Database" }
                p { class: "field__value", "library.db" }
                button { class: "btn btn--ghost", onclick: on_backup_db, "Backup database" }
            }

            if let Some(msg) = &snap_status {
                p { class: "wizard__success", "✓ {msg}" }
            }
            if let Some(err) = &snap_error {
                p { class: "wizard__error", "{err}" }
            }
        }
    }
}

fn walk_total_bytes(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total = total.saturating_add(walk_total_bytes(&path));
            } else if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

/// Recursively delete every file under `dir` but keep `dir` itself.
fn clear_dir(dir: &std::path::Path) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn format_bytes(b: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if b >= GB {
        format!("{:.2} GB", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.1} MB", b as f64 / MB as f64)
    } else if b >= KB {
        format!("{:.0} KB", b as f64 / KB as f64)
    } else {
        format!("{b} B")
    }
}
