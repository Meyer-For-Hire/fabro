use crate::RunSummaryStore;

pub(crate) async fn sqlite_summary_store() -> (tempfile::TempDir, RunSummaryStore) {
    let directory = tempfile::tempdir().unwrap();
    let database = fabro_db::Database::connect(directory.path().join("fabro.sqlite3"))
        .await
        .unwrap();
    database.migrate().await.unwrap();
    (directory, RunSummaryStore::new(database.clone_pool()))
}
