//! Integration tests for the single-file `.grafeo` database format.

#![cfg(feature = "grafeo-file")]

use grafeo_common::types::Value;
use grafeo_engine::{Config, GrafeoDB};

// =========================================================================
// Basic create, open, and reopen
// =========================================================================

#[test]
fn create_new_grafeo_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.grafeo");

    let db = GrafeoDB::with_config(Config::persistent(&path)).unwrap();
    assert_eq!(db.node_count(), 0);
    assert_eq!(db.edge_count(), 0);

    // File should exist
    assert!(path.exists());
    assert!(path.is_file());

    db.close().unwrap();
}

#[test]
fn insert_close_reopen_persists_data() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("persist.grafeo");

    // Create and populate
    {
        let db = GrafeoDB::with_config(Config::persistent(&path)).unwrap();
        let session = db.session();
        session
            .execute("INSERT (:Person {name: 'Alix', age: 30})")
            .unwrap();
        session
            .execute("INSERT (:Person {name: 'Gus', age: 25})")
            .unwrap();
        session
            .execute(
                "MATCH (a:Person {name: 'Alix'}), (b:Person {name: 'Gus'}) \
                 INSERT (a)-[:KNOWS]->(b)",
            )
            .unwrap();
        assert_eq!(db.node_count(), 2);
        assert_eq!(db.edge_count(), 1);
        db.close().unwrap();
    }

    // Sidecar WAL should be gone after close
    let wal_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".wal");
        std::path::PathBuf::from(p)
    };
    assert!(
        !wal_path.exists(),
        "sidecar WAL should be removed after close"
    );

    // Reopen and verify
    {
        let db = GrafeoDB::with_config(Config::persistent(&path)).unwrap();
        assert_eq!(db.node_count(), 2);
        assert_eq!(db.edge_count(), 1);

        // Verify data is queryable
        let session = db.session();
        let result = session
            .execute("MATCH (p:Person) RETURN p.name ORDER BY p.name")
            .unwrap();
        let mut names: Vec<String> = result
            .rows
            .iter()
            .filter_map(|r| match &r[0] {
                Value::String(s) => Some(s.to_string()),
                _ => None,
            })
            .collect();
        names.sort();
        assert_eq!(names, vec!["Alix", "Gus"]);
        db.close().unwrap();
    }
}

#[test]
fn save_as_grafeo_file_from_in_memory() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("exported.grafeo");

    // Create in-memory DB and populate
    let db = GrafeoDB::new_in_memory();
    let session = db.session();
    session
        .execute("INSERT (:City {name: 'Amsterdam'})")
        .unwrap();
    session.execute("INSERT (:City {name: 'Berlin'})").unwrap();
    assert_eq!(db.node_count(), 2);

    // Save as .grafeo file
    db.save(&path).unwrap();

    // Open the file and verify
    let db2 = GrafeoDB::with_config(Config::persistent(&path)).unwrap();
    assert_eq!(db2.node_count(), 2);

    let session2 = db2.session();
    let result = session2
        .execute("MATCH (c:City) RETURN c.name ORDER BY c.name")
        .unwrap();
    let mut names: Vec<String> = result
        .rows
        .iter()
        .filter_map(|r| match &r[0] {
            Value::String(s) => Some(s.to_string()),
            _ => None,
        })
        .collect();
    names.sort();
    assert_eq!(names, vec!["Amsterdam", "Berlin"]);
    db2.close().unwrap();
}

#[test]
fn wal_checkpoint_writes_to_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("checkpoint.grafeo");

    let db = GrafeoDB::with_config(Config::persistent(&path)).unwrap();
    let session = db.session();
    session
        .execute("INSERT (:Person {name: 'Vincent'})")
        .unwrap();

    // Checkpoint should write snapshot to file
    db.wal_checkpoint().unwrap();

    // Verify the file manager has a non-empty header
    let fm = db.file_manager().expect("should have file manager");
    let header = fm.active_header();
    assert!(header.snapshot_length > 0);
    assert_eq!(header.node_count, 1);
    assert_eq!(header.edge_count, 0);

    db.close().unwrap();
}

#[test]
fn multiple_checkpoints_alternate_headers() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("multi.grafeo");

    let db = GrafeoDB::with_config(Config::persistent(&path)).unwrap();
    let session = db.session();

    session.execute("INSERT (:Person {name: 'Jules'})").unwrap();
    db.wal_checkpoint().unwrap();

    let fm = db.file_manager().unwrap();
    assert_eq!(fm.active_header().iteration, 1);

    session.execute("INSERT (:Person {name: 'Mia'})").unwrap();
    db.wal_checkpoint().unwrap();
    assert_eq!(fm.active_header().iteration, 2);
    assert_eq!(fm.active_header().node_count, 2);

    db.close().unwrap();

    // Reopen and verify both nodes are there
    let db2 = GrafeoDB::with_config(Config::persistent(&path)).unwrap();
    assert_eq!(db2.node_count(), 2);
    db2.close().unwrap();
}

#[test]
fn auto_detect_does_not_use_grafeo_file_for_directory_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test_legacy");

    // Without .grafeo extension, should use WAL directory format
    let db = GrafeoDB::with_config(Config::persistent(&path)).unwrap();

    #[cfg(feature = "grafeo-file")]
    assert!(
        db.file_manager().is_none(),
        "directory path should not use single-file format"
    );

    let session = db.session();
    session.execute("INSERT (:Person {name: 'Butch'})").unwrap();
    db.close().unwrap();

    // Path should be a directory (WAL format)
    assert!(path.is_dir());
}

#[test]
fn info_reports_persistence() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("info.grafeo");

    let db = GrafeoDB::with_config(Config::persistent(&path)).unwrap();
    let info = db.info();
    assert!(info.is_persistent);
    assert!(info.path.is_some());
    db.close().unwrap();
}
