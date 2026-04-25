//! Concurrent download manager with resume support.
//!
//! The manager owns a tokio task pool of size `max_concurrent`. Each
//! worker pulls the oldest queued download, asks the source provider to
//! fetch it (with resume), and reports progress to both the DB and an
//! optional broadcast channel.

use crate::error::Result;
use crate::library::queries;
use crate::sources::{DownloadProgress, SourceProvider};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Semaphore, mpsc};

/// One progress update emitted by the manager.
#[derive(Debug, Clone)]
pub struct DownloadUpdate {
    /// The download row ID.
    pub download_id: String,
    /// The track being downloaded.
    pub track_id: String,
    /// Bytes received so far.
    pub bytes_done: u64,
    /// Total bytes if known.
    pub bytes_total: Option<u64>,
    /// Instantaneous speed in bytes/sec.
    pub speed_bps: Option<u64>,
    /// Final state (`done`, `failed`, etc.) — None means still in progress.
    pub final_state: Option<String>,
}

/// Concurrent download manager.
#[derive(Clone)]
pub struct DownloadManager {
    pool: SqlitePool,
    /// Active source registry — keyed by source ID.
    sources: Arc<HashMap<String, Arc<dyn SourceProvider>>>,
    /// Concurrency cap.
    semaphore: Arc<Semaphore>,
    /// Broadcast of progress events to UI subscribers.
    update_tx: mpsc::Sender<DownloadUpdate>,
}

impl DownloadManager {
    /// Construct.
    pub fn new(
        pool: SqlitePool,
        sources: HashMap<String, Arc<dyn SourceProvider>>,
        max_concurrent: usize,
        update_tx: mpsc::Sender<DownloadUpdate>,
    ) -> Self {
        Self {
            pool,
            sources: Arc::new(sources),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            update_tx,
        }
    }

    /// Enqueue a track for download. Returns the new download row ID.
    pub async fn enqueue(&self, track_id: &str, dest: &std::path::Path) -> Result<String> {
        let row = queries::downloads::enqueue(&self.pool, track_id, &dest.to_string_lossy()).await?;
        Ok(row.id)
    }

    /// Spawn a background task that processes queued downloads forever.
    /// Returns the JoinHandle so the caller can shut it down.
    pub fn spawn_worker_pool(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                // Throttle so we don't busy-loop when the queue is empty.
                let queued = match queries::downloads::claim_next_queued(&me.pool).await {
                    Ok(Some(d)) => d,
                    Ok(None) => {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "download queue read failed");
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                };

                let me2 = me.clone();
                tokio::spawn(async move {
                    let permit = me2.semaphore.clone().acquire_owned().await;
                    if let Ok(_p) = permit {
                        if let Err(e) = me2.run_one(&queued).await {
                            tracing::warn!(id = %queued.id, error = %e, "download failed");
                            let _ = queries::downloads::mark_failed(&me2.pool, &queued.id, &e.to_string()).await;
                            let _ = me2
                                .update_tx
                                .send(DownloadUpdate {
                                    download_id: queued.id,
                                    track_id: queued.track_id,
                                    bytes_done: queued.bytes_done as u64,
                                    bytes_total: queued.bytes_total.map(|t| t as u64),
                                    speed_bps: None,
                                    final_state: Some("failed".into()),
                                })
                                .await;
                        }
                    }
                });
            }
        })
    }

    /// Process a single download row: look up the track, find its source,
    /// stream to disk with progress reporting.
    async fn run_one(&self, d: &queries::downloads::Download) -> Result<()> {
        let track = queries::tracks::by_id(&self.pool, &d.track_id).await?;
        let source = self
            .sources
            .get(&track.source_id)
            .cloned()
            .ok_or_else(|| crate::error::SonitusError::Source {
                kind: "unknown",
                message: format!("source {} not registered", track.source_id),
            })?;

        let local = std::path::PathBuf::from(d.local_path.clone().unwrap_or_default());

        let (tx, mut rx) = mpsc::channel::<DownloadProgress>(32);
        let pool = self.pool.clone();
        let id = d.id.clone();
        let track_id = d.track_id.clone();
        let update_tx = self.update_tx.clone();
        let progress_relay = tokio::spawn(async move {
            while let Some(p) = rx.recv().await {
                let _ = queries::downloads::update_progress(
                    &pool,
                    &id,
                    p.bytes_done as i64,
                    p.bytes_total.map(|x| x as i64),
                    p.speed_bps.map(|x| x as i64),
                )
                .await;
                let _ = update_tx
                    .send(DownloadUpdate {
                        download_id: id.clone(),
                        track_id: track_id.clone(),
                        bytes_done: p.bytes_done,
                        bytes_total: p.bytes_total,
                        speed_bps: p.speed_bps,
                        final_state: None,
                    })
                    .await;
            }
        });

        source.download(&track.remote_path, &local, tx).await?;
        let _ = progress_relay.await;

        queries::downloads::mark_done(&self.pool, &d.id).await?;
        let _ = self
            .update_tx
            .send(DownloadUpdate {
                download_id: d.id.clone(),
                track_id: d.track_id.clone(),
                bytes_done: d.bytes_total.unwrap_or(0) as u64,
                bytes_total: d.bytes_total.map(|x| x as u64),
                speed_bps: None,
                final_state: Some("done".into()),
            })
            .await;
        Ok(())
    }

    /// Cancel a download.
    pub async fn cancel(&self, download_id: &str) -> Result<()> {
        queries::downloads::mark_cancelled(&self.pool, download_id).await
    }

    /// Pause a download.
    pub async fn pause(&self, download_id: &str) -> Result<()> {
        queries::downloads::mark_paused(&self.pool, download_id).await
    }

    /// Resume a paused download.
    pub async fn resume(&self, download_id: &str) -> Result<()> {
        queries::downloads::mark_resumed(&self.pool, download_id).await
    }

    /// Purge done/failed/cancelled downloads from the queue.
    pub async fn purge_terminal(&self) -> Result<u64> {
        queries::downloads::purge_terminal(&self.pool).await
    }
}
