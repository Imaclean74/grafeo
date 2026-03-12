"""End-to-end tests for the single-file .grafeo database format.

Tests persistence, checkpoint, and reopen through the Python binding layer.
"""

import tempfile
from pathlib import Path

from grafeo import GrafeoDB


class TestGrafeoFilePersistence:
    """Tests for .grafeo single-file format via Python bindings."""

    def test_create_and_reopen(self):
        """Data inserted in one session persists after close/reopen."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = str(Path(tmpdir) / "test.grafeo")

            # Create and populate
            db = GrafeoDB(path=db_path)
            session = db.session()
            session.execute("INSERT (:Person {name: 'Alix', age: 30})")
            session.execute("INSERT (:Person {name: 'Gus', age: 25})")
            session.execute(
                "MATCH (a:Person {name: 'Alix'}), (b:Person {name: 'Gus'}) INSERT (a)-[:KNOWS]->(b)"
            )
            info = db.info()
            assert info["node_count"] == 2
            assert info["edge_count"] == 1
            db.close()

            # Sidecar WAL should be removed after close
            wal_path = Path(db_path + ".wal")
            assert not wal_path.exists(), "sidecar WAL should be cleaned up"

            # Reopen and verify
            db2 = GrafeoDB.open(db_path)
            info2 = db2.info()
            assert info2["node_count"] == 2
            assert info2["edge_count"] == 1

            # Query the data
            session2 = db2.session()
            result = session2.execute("MATCH (p:Person) RETURN p.name ORDER BY p.name")
            names = sorted(row[0] for row in result)
            assert names == ["Alix", "Gus"]

            db2.close()

    def test_save_as_grafeo_file(self):
        """In-memory DB saved as .grafeo file can be reopened."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = str(Path(tmpdir) / "saved.grafeo")

            db = GrafeoDB()
            session = db.session()
            session.execute("INSERT (:City {name: 'Amsterdam'})")
            session.execute("INSERT (:City {name: 'Berlin'})")
            db.save(db_path)

            db2 = GrafeoDB.open(db_path)
            info = db2.info()
            assert info["node_count"] == 2

            session2 = db2.session()
            result = session2.execute("MATCH (c:City) RETURN c.name ORDER BY c.name")
            names = sorted(row[0] for row in result)
            assert names == ["Amsterdam", "Berlin"]
            db2.close()

    def test_multiple_reopen_cycles(self):
        """Data accumulates correctly across multiple open/close cycles."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = str(Path(tmpdir) / "cycles.grafeo")

            # Cycle 1
            db = GrafeoDB(path=db_path)
            db.session().execute("INSERT (:Person {name: 'Alix'})")
            db.close()

            # Cycle 2
            db = GrafeoDB.open(db_path)
            assert db.info()["node_count"] == 1
            db.session().execute("INSERT (:Person {name: 'Gus'})")
            db.close()

            # Cycle 3
            db = GrafeoDB.open(db_path)
            assert db.info()["node_count"] == 2
            db.session().execute("INSERT (:Person {name: 'Vincent'})")
            db.close()

            # Final check
            db = GrafeoDB.open(db_path)
            assert db.info()["node_count"] == 3
            result = db.session().execute("MATCH (p:Person) RETURN p.name")
            names = sorted(row[0] for row in result)
            assert names == ["Alix", "Gus", "Vincent"]
            db.close()

    def test_checkpoint_and_continued_writes(self):
        """Manual checkpoint followed by more writes all persist."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = str(Path(tmpdir) / "checkpoint.grafeo")

            db = GrafeoDB(path=db_path)
            session = db.session()

            session.execute("INSERT (:Person {name: 'Alix'})")
            db.wal_checkpoint()

            session.execute("INSERT (:Person {name: 'Gus'})")
            db.close()

            db2 = GrafeoDB.open(db_path)
            assert db2.info()["node_count"] == 2
            db2.close()

    def test_edges_with_properties_persist(self):
        """Edge properties survive close/reopen."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = str(Path(tmpdir) / "edge_props.grafeo")

            db = GrafeoDB(path=db_path)
            session = db.session()
            session.execute("INSERT (:Person {name: 'Alix'})")
            session.execute("INSERT (:Person {name: 'Gus'})")
            session.execute(
                "MATCH (a:Person {name: 'Alix'}), (b:Person {name: 'Gus'}) "
                "INSERT (a)-[:KNOWS {since: 2020}]->(b)"
            )
            db.close()

            db2 = GrafeoDB.open(db_path)
            session2 = db2.session()
            result = session2.execute("MATCH ()-[e:KNOWS]->() RETURN e.since")
            assert len(result) == 1
            assert result[0][0] == 2020
            db2.close()

    def test_named_graphs_persist(self):
        """Named graphs survive close/reopen."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = str(Path(tmpdir) / "named.grafeo")

            db = GrafeoDB(path=db_path)
            session = db.session()
            session.execute("CREATE GRAPH social")
            session.execute("USE GRAPH social")
            session.execute("INSERT (:Person {name: 'Alix'})")
            session.execute("USE GRAPH DEFAULT")
            session.execute("INSERT (:Person {name: 'Gus'})")
            db.close()

            db2 = GrafeoDB.open(db_path)
            session2 = db2.session()

            # Default graph has Gus
            result = session2.execute("MATCH (p:Person) RETURN p.name")
            assert sorted(row[0] for row in result) == ["Gus"]

            # Social graph has Alix
            session2.execute("USE GRAPH social")
            result = session2.execute("MATCH (p:Person) RETURN p.name")
            assert sorted(row[0] for row in result) == ["Alix"]
            db2.close()

    def test_file_is_single_file(self):
        """The .grafeo path should be a file, not a directory."""
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = Path(tmpdir) / "single.grafeo"

            db = GrafeoDB(path=str(db_path))
            db.session().execute("INSERT (:Node {x: 1})")
            db.close()

            assert db_path.is_file(), "should be a file, not a directory"
