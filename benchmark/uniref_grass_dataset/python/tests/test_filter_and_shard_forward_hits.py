import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import pandas as pd  # noqa: E402

from filter_and_shard_forward_hits import filter_fully_identical, shard_index  # noqa: E402


def _row(**overrides):
    base = dict(
        qseqid="Q1",
        sseqid="H1",
        pident=100.0,
        length=100,
        qstart=1,
        qend=100,
        qlen=100,
        sstart=1,
        send=100,
        slen=100,
        evalue=0.0,
        bitscore=200.0,
        stitle="some hit [Homo sapiens]",
        full_sseq="M" * 100,
    )
    base.update(overrides)
    return base


class TestFilterFullyIdentical(unittest.TestCase):
    def test_fully_identical_full_coverage_hit_is_dropped(self):
        df = pd.DataFrame([_row()])
        kept, n_dropped = filter_fully_identical(df)
        self.assertEqual(n_dropped, 1)
        self.assertEqual(len(kept), 0)

    def test_high_identity_but_partial_coverage_is_kept(self):
        df = pd.DataFrame([_row(qend=50)])  # only covers half the query
        kept, n_dropped = filter_fully_identical(df)
        self.assertEqual(n_dropped, 0)
        self.assertEqual(len(kept), 1)

    def test_full_coverage_but_below_identity_threshold_is_kept(self):
        df = pd.DataFrame([_row(pident=95.0)])
        kept, n_dropped = filter_fully_identical(df)
        self.assertEqual(n_dropped, 0)
        self.assertEqual(len(kept), 1)


class TestShardIndex(unittest.TestCase):
    def test_deterministic_across_calls(self):
        self.assertEqual(shard_index("Q12345", 200), shard_index("Q12345", 200))

    def test_always_within_range(self):
        for key in ["A", "B", "some_query_id", "UniRef50_Q9UM73"]:
            idx = shard_index(key, 17)
            self.assertGreaterEqual(idx, 0)
            self.assertLess(idx, 17)


if __name__ == "__main__":
    unittest.main()
