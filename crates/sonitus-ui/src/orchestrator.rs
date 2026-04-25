//! Application orchestrator — the glue that wires `sonitus-core` to the UI.
//!
//! On startup the orchestrator:
//!
//! 1. Loads the user's [`AppConfig`].
//! 2. Derives the [`VaultKey`] from the passphrase + salt.
//! 3. Opens the encrypted [`VaultDb`] and runs migrations.
//! 4. Constructs the [`AuditLogger`] and [`ConsentStore`].
//! 5. Loads the user's `.sonitus` library file.
//! 6. Builds a [`SourceProvider`] for each configured source, decrypting
//!    credentials on the fly.
//! 7. Spawns the [`DownloadManager`] worker pool.
//! 8. Spawns the player engine.
//! 9. Starts an event-pump task that translates `PlayerEvent`s into
//!    `Signal<PlayerState>` writes, `DownloadUpdate`s into
//!    `Signal<Vec<DownloadItem>>` writes, etc.
//!
//! The result is an [`AppHandle`] that components clone via `use_context`
//! to send commands and read state.

use crate::state::download_state::DownloadItem;
use crate::state::library_state::LibraryState;
use crate::state::player_state::PlayerState;
use dioxus::prelude::*;
use sonitus_core::config::AppConfig;
use sonitus_core::crypto::{VaultDb, VaultKey};
use sonitus_core::download::manager::{DownloadManager, DownloadUpdate};
use sonitus_core::error::{Result, SonitusError};
use sonitus_core::library::{Library, queries};
use sonitus_core::player::commands::PlayerCommand;
use sonitus_core::player::engine::{self, PlayerHandle, TrackResolver};
use sonitus_core::player::events::PlayerEvent;
use sonitus_core::privacy::{AuditLogger, ConsentStore};
use sonitus_core::sources::SourceProvider;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Top-level application services. Cheaply cloneable; underlying state is
/// reference-counted.
#[derive(Clone)]
pub struct AppHandle {
    /// User configuration, mirrored from `config.toml`.
    pub config: AppConfig,
    /// Encrypted SQLite vault.
    pub library: Library,
    /// Audit log + consent.
    pub audit: Arc<AuditLogger>,
    /// User-managed consent for opt-in features.
    pub consent: ConsentStore,
    /// Downloader worker pool.
    pub downloads: Arc<DownloadManager>,
    /// Audio player engine.
    pub player: PlayerHandle,
    /// All registered source providers, keyed by ID.
    pub sources: Arc<HashMap<String, Arc<dyn SourceProvider>>>,
}

impl AppHandle {
    /// Send a command to the player. Convenience wrapper.
    pub fn play(&self, track_id: impl Into<String>) {
        let _ = self.player.send(PlayerCommand::Play { track_id: track_id.into() });
    }
    /// Pause/resume the player based on current state.
    pub fn toggle_play_pause(&self, currently_paused: bool) {
        let cmd = if currently_paused { PlayerCommand::Resume } else { PlayerCommand::Pause };
        let _ = self.player.send(cmd);
    }
    /// Skip to the next track.
    pub fn next(&self) { let _ = self.player.send(PlayerCommand::Next); }
    /// Skip to the previous track.
    pub fn prev(&self) { let _ = self.player.send(PlayerCommand::Prev); }
    /// Seek to position in seconds.
    pub fn seek(&self, seconds: f64) { let _ = self.player.send(PlayerCommand::Seek { seconds }); }
    /// Set linear volume in `0.0..=1.0`.
    pub fn set_volume(&self, amplitude: f32) {
        let _ = self.player.send(PlayerCommand::SetVolume { amplitude });
    }
    /// Append to the queue.
    pub fn enqueue(&self, track_id: impl Into<String>) {
        let _ = self.player.send(PlayerCommand::Enqueue { track_id: track_id.into() });
    }
}

/// Configuration parameters for [`boot`].
pub struct BootConfig {
    /// User-supplied passphrase used to derive the vault key.
    pub passphrase: String,
}

/// Channels that emerge from boot — consumed by [`start_event_pump`].
///
/// Held separately from [`AppHandle`] because they are single-consumer:
/// they move into the pump task, leaving `AppHandle` cheaply cloneable.
pub struct BootChannels {
    /// Stream of download progress updates from the manager's workers.
    pub downloads: mpsc::Receiver<DownloadUpdate>,
}

/// Boot the application, returning a wired [`AppHandle`] and the matching
/// [`BootChannels`]. The UI calls this during startup; all errors propagate
/// upward so the unlock screen can show them.
pub async fn boot(boot_cfg: BootConfig) -> Result<(AppHandle, BootChannels)> {
    let app_config = AppConfig::load()?;

    // Derive the vault key from the passphrase + persisted salt.
    let salt_path = AppConfig::vault_salt_path()?;
    let salt = VaultKey::load_or_generate_salt(&salt_path)?;
    let key = VaultKey::derive(&boot_cfg.passphrase, &salt)?;

    // Open the encrypted DB.
    let db_path = AppConfig::db_path()?;
    let vault = VaultDb::open(&db_path, key).await?;
    let library = Library::new(vault);

    // Audit logger + consent store.
    let audit = Arc::new(AuditLogger::new(
        AppConfig::audit_log_path()?,
        app_config.audit_log_max_mb,
        app_config.audit_log_keep_rotations,
    )?);
    let consent_path = AppConfig::config_dir()?.join("consent.toml");
    let consent = ConsentStore::load(consent_path)?;

    // Build source providers from DB rows.
    let sources = build_source_registry(&library, audit.clone()).await?;
    let sources = Arc::new(sources);

    // Download manager + worker pool.
    let (dl_tx, dl_rx) = mpsc::channel::<DownloadUpdate>(256);
    let download_mgr = Arc::new(DownloadManager::new(
        library.pool().clone(),
        (*sources).clone(),
        app_config.max_concurrent_downloads,
        dl_tx,
    ));
    download_mgr.spawn_worker_pool();

    // Player engine.
    let rt_handle = tokio::runtime::Handle::current();
    let resolver: Arc<dyn TrackResolver> = Arc::new(LibraryTrackResolver::new(
        library.clone(),
        AppConfig::cache_dir()?,
        sources.clone(),
        rt_handle,
    ));
    let player = engine::spawn(resolver);

    let handle = AppHandle {
        config: app_config,
        library,
        audit,
        consent,
        downloads: download_mgr,
        player,
        sources,
    };
    let channels = BootChannels { downloads: dl_rx };
    Ok((handle, channels))
}

/// Resolve a track ID into a [`Track`] + a playable local file path.
///
/// If the track has already been cached locally, returns that path.
/// Otherwise asks the source provider for the local path (LocalSource
/// joins root + relative). Streaming sources without a local path must
/// be downloaded into the offline cache before play.
struct LibraryTrackResolver {
    library: Library,
    cache_dir: std::path::PathBuf,
    sources: Arc<HashMap<String, Arc<dyn SourceProvider>>>,
    /// Captured at boot. The decode thread is a `std::thread::spawn` (not
    /// a Tokio worker), so `futures::executor::block_on` panics inside
    /// sqlx. We use this Handle to enter the Tokio runtime context for
    /// the duration of each DB lookup.
    rt_handle: tokio::runtime::Handle,
}

impl LibraryTrackResolver {
    fn new(
        library: Library,
        cache_dir: std::path::PathBuf,
        sources: Arc<HashMap<String, Arc<dyn SourceProvider>>>,
        rt_handle: tokio::runtime::Handle,
    ) -> Self {
        Self { library, cache_dir, sources, rt_handle }
    }
}

impl TrackResolver for LibraryTrackResolver {
    fn resolve(&self, track_id: &str) -> Result<(sonitus_core::library::Track, std::path::PathBuf)> {
        let pool = self.library.pool().clone();
        let tid = track_id.to_string();
        // Enter the Tokio runtime context so sqlx can find its reactor.
        let track = self.rt_handle.block_on(async move {
            queries::tracks::by_id(&pool, &tid).await
        })?;

        // Resolve the absolute file path:
        //   1. Local cache (downloaded copy) wins if set.
        //   2. Otherwise ask the source provider — LocalSource joins
        //      root + relative; cloud sources return None.
        //   3. Fall back to the BLAKE3-keyed cache location for files
        //      that have been hashed but not downloaded.
        let path = if let Some(p) = track.local_cache_path.as_ref().filter(|s| !s.is_empty()) {
            std::path::PathBuf::from(p)
        } else if let Some(source) = self.sources.get(&track.source_id) {
            source.local_path(&track.remote_path).ok_or_else(|| {
                crate::orchestrator::resolve_error(format!(
                    "source '{}' is remote; download the track first",
                    track.source_id
                ))
            })?
        } else if let Some(hash) = &track.content_hash {
            sonitus_core::library::models::cache_path_for(&self.cache_dir, hash)
        } else {
            return Err(crate::orchestrator::resolve_error(format!(
                "track {} has no local path and no content hash",
                track_id
            )));
        };
        Ok((track, path))
    }
}

/// Helper: typed error for path-resolution failures.
pub(crate) fn resolve_error(msg: String) -> sonitus_core::error::SonitusError {
    sonitus_core::error::SonitusError::Audio(msg)
}

/// If the engine reports a duration that differs from what the DB row
/// has (typical case: CBR mp3 where the scanner couldn't get duration
/// from headers), backfill the DB so the tracks list shows the correct
/// time without re-playing.
async fn persist_duration_if_new(
    library: &sonitus_core::library::Library,
    track: &sonitus_core::library::Track,
    discovered_ms: u64,
    library_signal: &mut Signal<LibraryState>,
) {
    let pool = library.pool();
    if discovered_ms == 0 {
        return;
    }
    let stored = track.duration_ms.unwrap_or(0).max(0) as u64;
    // Only update if missing or differs by more than a second
    // (avoid churn from rounding noise).
    if stored == 0 || stored.abs_diff(discovered_ms) > 1_000 {
        if let Err(e) = sonitus_core::library::queries::tracks::set_duration_ms(
            pool,
            &track.id,
            discovered_ms as i64,
        )
        .await
        {
            tracing::warn!(error = %e, "failed to persist track duration");
        } else {
            tracing::info!(
                track_id = %track.id,
                discovered_ms,
                "persisted discovered track duration"
            );
            // Bump the library version so any open use_resource that
            // depends on it re-runs and picks up the new duration.
            let next = library_signal.peek().version.wrapping_add(1);
            library_signal.write().version = next;
        }
    }
}

/// Build a `SourceProvider` for every row in the `sources` table, decrypting
/// stored OAuth credentials as we go.
async fn build_source_registry(
    library: &Library,
    audit: Arc<AuditLogger>,
) -> Result<HashMap<String, Arc<dyn SourceProvider>>> {
    use sonitus_core::library::models::SourceKind;
    use std::str::FromStr;

    let rows = queries::sources::list_enabled(library.pool()).await?;
    let mut out: HashMap<String, Arc<dyn SourceProvider>> = HashMap::new();

    for row in rows {
        let kind = match SourceKind::from_str(&row.kind) {
            Ok(k) => k,
            Err(_) => {
                tracing::warn!(source_id = %row.id, kind = %row.kind, "skipping unknown source kind");
                continue;
            }
        };

        let provider: Arc<dyn SourceProvider> = match kind {
            SourceKind::Local => {
                let cfg: serde_json::Value = serde_json::from_str(&row.config_json)?;
                let path = cfg
                    .get("path")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SonitusError::Source { kind: "local", message: "missing path".into() })?;
                Arc::new(sonitus_core::sources::local::LocalSource::new(
                    row.id.clone(),
                    row.name.clone(),
                    std::path::PathBuf::from(path),
                ))
            }
            SourceKind::Http => {
                let cfg: serde_json::Value = serde_json::from_str(&row.config_json)?;
                let url = cfg
                    .get("url")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| SonitusError::Source { kind: "http", message: "missing url".into() })?;
                let parsed = url::Url::parse(url)
                    .map_err(|e| SonitusError::Source { kind: "http", message: e.to_string() })?;
                Arc::new(sonitus_core::sources::http::HttpSource::new(
                    row.id.clone(),
                    row.name.clone(),
                    parsed,
                    audit.clone(),
                ))
            }
            SourceKind::GoogleDrive => {
                // Decrypt credentials.
                let creds = queries::sources::read_credentials(library.vault(), &row.id).await?;
                let Some(creds) = creds else {
                    tracing::warn!(source_id = %row.id, "drive source has no credentials; skipping");
                    continue;
                };
                let cfg: serde_json::Value = serde_json::from_str(&row.config_json)?;
                let root_folder = cfg.get("root_folder").and_then(|v| v.as_str()).map(str::to_string);
                let client_id = cfg.get("client_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let client_secret = cfg.get("client_secret").and_then(|v| v.as_str()).unwrap_or("").to_string();
                Arc::new(sonitus_core::sources::google_drive::GoogleDriveSource::new(
                    row.id.clone(),
                    row.name.clone(),
                    root_folder,
                    creds.primary.clone(),
                    creds.secondary.clone(),
                    creds.expires_at,
                    client_id,
                    client_secret,
                    audit.clone(),
                ))
            }
            SourceKind::Dropbox => {
                let creds = queries::sources::read_credentials(library.vault(), &row.id).await?;
                let Some(creds) = creds else { continue; };
                let cfg: serde_json::Value = serde_json::from_str(&row.config_json)?;
                Arc::new(sonitus_core::sources::dropbox::DropboxSource::new(
                    row.id.clone(),
                    row.name.clone(),
                    creds.primary.clone(),
                    creds.secondary.clone(),
                    creds.expires_at,
                    cfg.get("client_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    cfg.get("client_secret").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    audit.clone(),
                ))
            }
            SourceKind::Onedrive => {
                let creds = queries::sources::read_credentials(library.vault(), &row.id).await?;
                let Some(creds) = creds else { continue; };
                let cfg: serde_json::Value = serde_json::from_str(&row.config_json)?;
                Arc::new(sonitus_core::sources::onedrive::OneDriveSource::new(
                    row.id.clone(),
                    row.name.clone(),
                    creds.primary.clone(),
                    creds.secondary.clone(),
                    creds.expires_at,
                    cfg.get("client_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    cfg.get("client_secret").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    cfg.get("tenant").and_then(|v| v.as_str()).unwrap_or("common").to_string(),
                    audit.clone(),
                ))
            }
            #[cfg(feature = "s3")]
            SourceKind::S3 => {
                let creds = queries::sources::read_credentials(library.vault(), &row.id).await?;
                let Some(creds) = creds else { continue; };
                let cfg: serde_json::Value = serde_json::from_str(&row.config_json)?;
                Arc::new(
                    sonitus_core::sources::s3::S3Source::new(
                        row.id.clone(),
                        row.name.clone(),
                        cfg.get("bucket").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        cfg.get("prefix").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        creds.primary.clone(),
                        creds.secondary.clone().unwrap_or_default(),
                        cfg.get("region").and_then(|v| v.as_str()).unwrap_or("us-east-1").to_string(),
                        cfg.get("endpoint_url").and_then(|v| v.as_str()).map(str::to_string),
                        audit.clone(),
                    )
                    .await?,
                )
            }
            #[cfg(not(feature = "s3"))]
            SourceKind::S3 => {
                tracing::warn!(source_id = %row.id, "s3 feature disabled; skipping source");
                continue;
            }
            #[cfg(feature = "smb")]
            SourceKind::Smb => {
                let creds = queries::sources::read_credentials(library.vault(), &row.id).await?;
                let Some(creds) = creds else { continue; };
                let cfg: serde_json::Value = serde_json::from_str(&row.config_json)?;
                Arc::new(sonitus_core::sources::smb::SmbSource::new(
                    row.id.clone(),
                    row.name.clone(),
                    cfg.get("host").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    cfg.get("share").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    cfg.get("base_path").and_then(|v| v.as_str()).unwrap_or("/").to_string(),
                    creds.primary.clone(),
                    creds.secondary.clone().unwrap_or_default(),
                    audit.clone(),
                ))
            }
            #[cfg(not(feature = "smb"))]
            SourceKind::Smb => {
                tracing::warn!(source_id = %row.id, "smb feature disabled; skipping source");
                continue;
            }
        };

        out.insert(row.id, provider);
    }
    Ok(out)
}

/// Spawn a tokio task that pumps player + download events into the UI Signals.
///
/// Call this after [`boot`] returns and Signals have been installed via
/// [`crate::state::install_player_state`] etc. The pump runs forever; the
/// returned `JoinHandle` is fire-and-forget — drop it to let the pump
/// continue, or `abort()` it on shutdown.
pub fn start_event_pump(
    handle: AppHandle,
    mut channels: BootChannels,
    mut player_state: Signal<PlayerState>,
    mut downloads_state: Signal<Vec<DownloadItem>>,
    mut library_state: Signal<LibraryState>,
) {
    // Dioxus 0.7 Signals use UnsyncStorage (RefCell-backed) and are not Send.
    // `dioxus::prelude::spawn` runs the task on Dioxus's local executor where
    // Send isn't required, so we can capture Signals across .await points.
    dioxus::prelude::spawn(async move {
        // Refresh the library state once on startup.
        if let Ok(summary) = handle.library.summary().await {
            let mut s = library_state.write();
            s.track_count = summary.tracks;
            s.album_count = summary.albums;
            s.artist_count = summary.artists;
            s.playlist_count = summary.playlists;
        }
        if let Ok(sources) = queries::sources::list_all(handle.library.pool()).await {
            library_state.write().sources = sources;
        }

        // The player engine writes to a `crossbeam_channel::Receiver`,
        // which is sync; we poll it on a tokio interval so we can also
        // await on the async download channel cooperatively.
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(33));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    while let Some(evt) = handle.player.try_next_event() {
                        // Pre-process Playing events to backfill the DB
                        // when the engine discovered the track's true
                        // duration (e.g. CBR mp3 packet walk).
                        if let PlayerEvent::Playing { ref track, duration_ms } = evt {
                            persist_duration_if_new(
                                &handle.library,
                                track,
                                duration_ms,
                                &mut library_state,
                            ).await;
                        }
                        apply_player_event(&mut player_state, evt);
                    }
                }
                Some(update) = channels.downloads.recv() => {
                    apply_download_update(&mut downloads_state, update);
                }
            }
        }
    });
}

fn apply_download_update(state: &mut Signal<Vec<DownloadItem>>, update: DownloadUpdate) {
    let mut list = state.write();
    let pos = list.iter().position(|i| i.id == update.download_id);
    let progress = match update.bytes_total {
        Some(t) if t > 0 => (update.bytes_done as f64) / (t as f64),
        _ => 0.0,
    };
    let item = DownloadItem {
        id: update.download_id.clone(),
        track_id: update.track_id.clone(),
        track_title: pos
            .and_then(|i| list.get(i))
            .map(|i| i.track_title.clone())
            .unwrap_or_else(|| update.track_id.clone()),
        status: update.final_state.clone().unwrap_or_else(|| "downloading".into()),
        progress,
        bytes_done: update.bytes_done,
        bytes_total: update.bytes_total,
        speed_bps: update.speed_bps,
        error: None,
    };
    match pos {
        Some(i) => list[i] = item,
        None => list.push(item),
    }
}

fn apply_player_event(state: &mut Signal<PlayerState>, evt: PlayerEvent) {
    let mut s = state.write();
    match evt {
        PlayerEvent::Playing { track, duration_ms } => {
            s.track = Some(track);
            s.duration_ms = duration_ms;
            s.position_ms = 0;
            s.is_paused = false;
            s.last_error = None;
        }
        PlayerEvent::Paused { position_ms } => {
            s.is_paused = true;
            s.position_ms = position_ms;
        }
        PlayerEvent::Resumed { position_ms } => {
            s.is_paused = false;
            s.position_ms = position_ms;
        }
        PlayerEvent::Stopped => {
            s.track = None;
            s.is_paused = false;
            s.position_ms = 0;
            s.duration_ms = 0;
        }
        PlayerEvent::Progress { position_ms, duration_ms, .. } => {
            s.position_ms = position_ms;
            s.duration_ms = duration_ms;
        }
        PlayerEvent::TrackEnded { .. } => {
            // Next track event will arrive in turn.
        }
        PlayerEvent::QueueChanged { queue } => {
            s.queue = queue;
        }
        PlayerEvent::VolumeChanged { amplitude } => {
            s.volume = amplitude;
        }
        PlayerEvent::OutputDeviceChanged { device_name } => {
            s.output_device = device_name;
        }
        PlayerEvent::Error { message } => {
            s.last_error = Some(message);
        }
    }
}
