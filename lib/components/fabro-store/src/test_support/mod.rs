#[cfg(test)]
use std::path::Path;

use fabro_types::RunId;

#[cfg(test)]
use crate::RunSummaryStore;
use crate::{Database, Result};

/// Writes an event without append validation to model a log corrupted by an
/// older Fabro version.
pub async fn put_unvalidated_run_event(
    database: &Database,
    run_id: &RunId,
    seq: u32,
    payload: &serde_json::Value,
) -> Result<()> {
    database
        .put_unvalidated_run_event(run_id, seq, payload)
        .await
}

#[cfg(test)]
pub(crate) async fn sqlite_summary_store() -> (tempfile::TempDir, RunSummaryStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = sqlite_summary_store_at(directory.path()).await;
    (directory, store)
}

#[cfg(test)]
pub(crate) async fn sqlite_summary_store_at(directory: &Path) -> RunSummaryStore {
    let database = fabro_db::Database::connect(directory.join("fabro.sqlite3"))
        .await
        .unwrap();
    database.migrate().await.unwrap();
    RunSummaryStore::new(database.clone_pool())
}
