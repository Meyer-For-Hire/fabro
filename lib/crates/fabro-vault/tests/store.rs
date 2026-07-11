#![expect(
    clippy::unwrap_used,
    reason = "SQLite secret-store integration tests use panic-on-failure fixture setup"
)]

use std::collections::HashMap;

use chrono::{TimeZone as _, Utc};
use fabro_db::Database;
use fabro_types::SecretType;
use fabro_vault::{SecretEntry, SecretStore, SecretStoreError, import_legacy_json_once};
use tokio::fs;

async fn test_database() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let database = Database::connect(dir.path().join("fabro.sqlite3"))
        .await
        .unwrap();
    database.migrate().await.unwrap();
    (dir, database)
}

#[tokio::test]
async fn crud_preserves_metadata_and_description() {
    let (_dir, database) = test_database().await;
    let store = SecretStore::new(database.clone_pool());

    let created = store
        .set(
            "OPENAI_API_KEY",
            "first",
            SecretType::Token,
            Some("provider key"),
        )
        .await
        .unwrap();
    let updated = store
        .set("OPENAI_API_KEY", "second", SecretType::Token, None)
        .await
        .unwrap();

    assert_eq!(created.created_at, updated.created_at);
    assert_eq!(updated.description.as_deref(), Some("provider key"));
    let entry = store.get("OPENAI_API_KEY").await.unwrap().unwrap();
    assert_eq!(entry.value, "second");
    assert_eq!(entry.revision, 2);
    let listed = store.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, updated.name);
    assert_eq!(listed[0].description, updated.description);

    store.remove("OPENAI_API_KEY").await.unwrap();
    assert!(store.get("OPENAI_API_KEY").await.unwrap().is_none());
}

#[tokio::test]
async fn independent_stores_observe_writes() {
    let (_dir, database) = test_database().await;
    let first = SecretStore::new(database.clone_pool());
    let second = SecretStore::new(database.clone_pool());

    first
        .set("ANTHROPIC_API_KEY", "key", SecretType::Token, None)
        .await
        .unwrap();

    assert_eq!(
        second
            .get("ANTHROPIC_API_KEY")
            .await
            .unwrap()
            .unwrap()
            .value,
        "key"
    );
}

#[tokio::test]
async fn replace_if_revision_rejects_stale_writer() {
    let (_dir, database) = test_database().await;
    let store = SecretStore::new(database.clone_pool());
    store
        .set("OPENAI_CODEX", "{}", SecretType::Oauth, None)
        .await
        .unwrap();

    let updated = store
        .replace_if_revision(
            "OPENAI_CODEX",
            1,
            r#"{"token":"winner"}"#,
            SecretType::Oauth,
        )
        .await
        .unwrap();
    assert_eq!(updated.revision, 2);

    let err = store
        .replace_if_revision("OPENAI_CODEX", 1, r#"{"token":"loser"}"#, SecretType::Oauth)
        .await
        .unwrap_err();
    assert!(matches!(err, SecretStoreError::StaleRevision {
        expected: 1,
        actual: 2,
        ..
    }));
}

#[tokio::test]
async fn imports_legacy_json_once_without_overwriting_sql() {
    let (dir, database) = test_database().await;
    let store = SecretStore::new(database.clone_pool());
    store
        .set("EXISTING_KEY", "sql", SecretType::Token, None)
        .await
        .unwrap();
    let timestamp = Utc.with_ymd_and_hms(2026, 7, 11, 12, 0, 0).unwrap();
    let entries = HashMap::from([
        ("EXISTING_KEY".to_string(), SecretEntry {
            value:       "legacy".to_string(),
            secret_type: SecretType::Token,
            description: None,
            created_at:  timestamp,
            updated_at:  timestamp,
            revision:    1,
        }),
        ("NEW_KEY".to_string(), SecretEntry {
            value:       "new".to_string(),
            secret_type: SecretType::Token,
            description: Some("imported".to_string()),
            created_at:  timestamp,
            updated_at:  timestamp,
            revision:    1,
        }),
    ]);
    let source = dir.path().join("secrets.json");
    fs::write(&source, serde_json::to_vec(&entries).unwrap())
        .await
        .unwrap();

    let report = import_legacy_json_once(database.pool(), &source)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(report.imported_rows, 1);
    assert_eq!(report.skipped_rows, 1);
    assert!(!source.exists());
    assert!(report.backup_path.exists());
    assert_eq!(
        store.get("EXISTING_KEY").await.unwrap().unwrap().value,
        "sql"
    );
    assert_eq!(store.get("NEW_KEY").await.unwrap().unwrap().value, "new");
    assert!(
        import_legacy_json_once(database.pool(), &source)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn malformed_legacy_json_does_not_import_or_rename() {
    let (dir, database) = test_database().await;
    let source = dir.path().join("secrets.json");
    fs::write(&source, br#"{"VALID_KEY":{"value":"secret"}}"#)
        .await
        .unwrap();

    let err = import_legacy_json_once(database.pool(), &source)
        .await
        .unwrap_err();

    assert!(matches!(err, SecretStoreError::LegacyParse { .. }));
    assert!(source.exists());
    assert!(
        SecretStore::new(database.clone_pool())
            .list()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn github_private_key_file_secret_satisfies_schema() {
    let (_dir, database) = test_database().await;
    let store = SecretStore::new(database.clone_pool());

    store
        .set("GITHUB_APP_PRIVATE_KEY", "pem", SecretType::File, None)
        .await
        .unwrap();
    store
        .set("/run/secrets/key.pem", "pem", SecretType::File, None)
        .await
        .unwrap();
}

#[test]
fn debug_redacts_secret_value() {
    let timestamp = Utc.with_ymd_and_hms(2026, 7, 11, 12, 0, 0).unwrap();
    let entry = SecretEntry {
        value:       "do-not-print".to_string(),
        secret_type: SecretType::Token,
        description: None,
        created_at:  timestamp,
        updated_at:  timestamp,
        revision:    1,
    };

    let rendered = format!("{entry:?}");
    assert!(!rendered.contains("do-not-print"));
    assert!(rendered.contains("[REDACTED]"));
}
