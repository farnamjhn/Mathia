//! In-app updates from GitHub Releases.
//!
//! Every release workflow run signs the bundles and publishes a `latest.json`
//! manifest next to them. The updater plugin reads that manifest from
//! `releases/latest/download/latest.json`, so a check always sees the newest
//! *published* release: drafts and prereleases are ignored by GitHub.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(debug_assertions))]
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

/// How often a running app looks for a newer release.
#[cfg(not(debug_assertions))]
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// Let the window appear before the first automatic check.
#[cfg(not(debug_assertions))]
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(5);

/// What `check_for_update` reports to the frontend.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// Emitted as `update-progress` while a release downloads.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: f32,
}

/// Looks for a newer published release. `Ok(None)` means the app is current.
#[cfg_attr(debug_assertions, allow(dead_code))]
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    match find_update(app).await {
        Ok(Some(update)) => Ok(Some(info(update))),
        Ok(None) => Ok(None),
        // No manifest means no release has been published with the updater
        // pipeline yet; that is not an error for the background check.
        Err(QueryError::NoManifest) => Ok(None),
        Err(QueryError::Other(message)) => Err(message),
    }
}

/// Like [`check`], but a missing manifest is reported instead of being treated
/// as "up to date", so a manual check explains what is wrong.
pub async fn check_manual(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    match find_update(app).await {
        Ok(Some(update)) => Ok(Some(info(update))),
        Ok(None) => Ok(None),
        Err(QueryError::NoManifest) => Err(
            "No update manifest (latest.json) was found in the latest published release. \
             Publish a release built with the auto-update workflow, then try again."
                .into(),
        ),
        Err(QueryError::Other(message)) => Err(message),
    }
}

/// Why a check could not produce a release.
enum QueryError {
    /// The endpoint answered, but without a usable `latest.json`.
    NoManifest,
    Other(String),
}

async fn find_update(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, QueryError> {
    let updater = app
        .updater()
        .map_err(|error| QueryError::Other(error.to_string()))?;
    match updater.check().await {
        Ok(update) => Ok(update),
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => Err(QueryError::NoManifest),
        Err(error) => Err(QueryError::Other(error.to_string())),
    }
}

fn info(update: tauri_plugin_updater::Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version,
        current_version: update.current_version,
        notes: update.body,
        date: update.date.map(|date| date.to_string()),
    }
}

/// Downloads the newest release, installs it and restarts the app. Never
/// returns on success: the process is replaced.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let Some(update) = find_update(app).await.map_err(|error| match error {
        QueryError::NoManifest => {
            "No update manifest (latest.json) was found in the latest published release.".into()
        }
        QueryError::Other(message) => message,
    })?
    else {
        return Err("Mathia is already up to date.".into());
    };

    let downloaded = Arc::new(AtomicU64::new(0));
    let handle = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                let done = downloaded.fetch_add(chunk as u64, Ordering::Relaxed) + chunk as u64;
                let percent = total
                    .filter(|total| *total > 0)
                    .map(|total| (done as f32 / total as f32).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                let _ = handle.emit(
                    "update-progress",
                    UpdateProgress {
                        downloaded: done,
                        total,
                        percent,
                    },
                );
            },
            || {},
        )
        .await
        .map_err(|error| error.to_string())?;

    app.restart();
}

/// Starts the background loop that keeps looking for newer releases. It
/// notifies the frontend once per version, so a dismissed prompt does not
/// reappear until a newer release exists.
#[cfg(not(debug_assertions))]
pub fn spawn_periodic_check(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        let mut notified: Option<String> = None;
        loop {
            match tauri::async_runtime::block_on(check(&app)) {
                Ok(Some(info)) => {
                    if notified.as_deref() != Some(info.version.as_str()) {
                        notified = Some(info.version.clone());
                        let _ = app.emit("update-available", info);
                    }
                }
                Ok(None) => {}
                Err(error) => eprintln!("update check failed: {error}"),
            }
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}
