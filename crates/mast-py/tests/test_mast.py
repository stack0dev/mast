"""Basic tests for mast-memory Python bindings."""
import os
import tempfile
import pytest

# These tests require the native module to be built first via `maturin develop`
try:
    from mast import Mast
    HAS_MAST = True
except ImportError:
    HAS_MAST = False


class DummyEmbedder:
    """A simple embedder that returns deterministic hash-based vectors."""

    def __init__(self, dims=32):
        self._dims = dims

    def embed(self, text: str) -> list[float]:
        h = hash(text) & 0xFFFFFFFF
        vec = []
        for i in range(self._dims):
            h = (h * 6364136223846793005 + 1442695040888963407) & 0xFFFFFFFFFFFFFFFF
            vec.append((h / (2**64)) * 2 - 1)
        # Normalize
        norm = sum(x * x for x in vec) ** 0.5
        return [x / norm for x in vec] if norm > 0 else vec

    def embed_batch(self, texts: list[str]) -> list[list[float]]:
        return [self.embed(t) for t in texts]

    def dimensions(self) -> int:
        return self._dims


@pytest.mark.skipif(not HAS_MAST, reason="mast native module not built")
class TestMast:
    def test_store_and_recall(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = os.path.join(tmpdir, "test.mast")
            m = Mast(db_path)
            emb = DummyEmbedder()

            mem = m.store("test", "hello world", emb)
            assert mem.id is not None
            assert mem.content == "hello world"

            results = m.recall("test", emb, query="hello")
            assert len(results) > 0
            assert results[0].memory.content == "hello world"
            m.close()

    def test_get_and_delete(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = os.path.join(tmpdir, "test.mast")
            m = Mast(db_path)
            emb = DummyEmbedder()

            mem = m.store("test", "to delete", emb)
            found = m.get("test", mem.id)
            assert found is not None
            assert found.content == "to delete"

            m.delete("test", mem.id)
            assert m.get("test", mem.id) is None
            m.close()

    def test_info(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = os.path.join(tmpdir, "test.mast")
            m = Mast(db_path)
            emb = DummyEmbedder()

            m.store("col_a", "first", emb)
            m.store("col_a", "second", emb)

            info = m.info()
            assert len(info) == 1
            assert info[0].name == "col_a"
            assert info[0].memory_count == 2
            m.close()

    def test_relate_and_traverse(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = os.path.join(tmpdir, "test.mast")
            m = Mast(db_path)

            edge_id = m.relate("test", "alice", "bob", "knows", weight=0.9)
            assert edge_id is not None

            edges = m.traverse("test", "alice")
            assert len(edges) == 1
            assert edges[0].target == "bob"
            m.close()

    def test_snapshot_and_restore(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            db1 = os.path.join(tmpdir, "db1.mast")
            m1 = Mast(db1)
            emb = DummyEmbedder()

            m1.store("snap", "memory 1", emb)
            m1.store("snap", "memory 2", emb)

            snap_path = os.path.join(tmpdir, "snapshot.jsonl")
            result = m1.snapshot("snap", snap_path)
            assert result.memory_count == 2
            m1.close()

            db2 = os.path.join(tmpdir, "db2.mast")
            m2 = Mast(db2)
            restore = m2.restore(snap_path)
            assert restore.memory_count == 2

            memories = m2.list("snap")
            assert len(memories) == 2
            m2.close()
