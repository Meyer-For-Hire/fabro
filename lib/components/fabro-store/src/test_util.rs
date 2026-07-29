use std::path::Path;

use crate::RunSummaryStore;

pub(crate) async fn sqlite_summary_store() -> (tempfile::TempDir, RunSummaryStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = sqlite_summary_store_at(directory.path()).await;
    (directory, store)
}

pub(crate) async fn sqlite_summary_store_at(directory: &Path) -> RunSummaryStore {
    let database = fabro_db::Database::connect(directory.join("fabro.sqlite3"))
        .await
        .unwrap();
    database.migrate().await.unwrap();
    RunSummaryStore::new(database.clone_pool())
}
