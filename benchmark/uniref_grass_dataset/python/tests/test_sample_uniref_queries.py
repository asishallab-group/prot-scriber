import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from sample_uniref_queries import (  # noqa: E402
    index_header_offsets,
    read_record_at,
    sample_clusters,
)

FIXTURE_FASTA = """\
>UniRef50_A00001 First protein n=2 Tax=Species one TaxID=1 RepID=A00001_SPEC1
MKTAYIAKQ
RQISFVKSH
>UniRef50_UPI000000AAAA Uncharacterized protein n=1 Tax=Species two TaxID=2 RepID=UPI000000AAAA
MSTNPKPQRKTKRNTNRRPQD
>UniRef50_A00002 Second protein n=3 Tax=Species three TaxID=3 RepID=A00002_SPEC3
MADEEKLPPGWEKRMSRSSG
>UniRef50_A00003 Third protein n=1 Tax=Species four TaxID=4 RepID=A00003_SPEC4
MSEQVDAAHDDTVSVAAAAKK
>UniRef50_A00004 Fourth protein n=2 Tax=Species five TaxID=5 RepID=A00004_SPEC5
MAAAAAAAAAAAAAAAAAAAA
"""


class TestSampleClusters(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.fasta_path = Path(self.tmpdir.name) / "uniref50.fasta"
        self.fasta_path.write_text(FIXTURE_FASTA)

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_multiline_sequence_is_concatenated(self):
        offsets = index_header_offsets(self.fasta_path)
        with open(self.fasta_path, "rb") as fh:
            header_line, sequence = read_record_at(fh, offsets[0])
        self.assertTrue(header_line.startswith(">UniRef50_A00001"))
        self.assertEqual(sequence, "MKTAYIAKQRQISFVKSH")

    def test_upi_representatives_are_excluded(self):
        # seed=0 happens to shuffle the UPI-only record ahead of the last of the 4 valid
        # ones, so it's guaranteed to actually be encountered (and skipped) rather than the
        # loop exiting early having collected all 4 valid clusters without ever reaching it.
        selected, total, n_upi_skipped, n_unparsable = sample_clusters(
            self.fasta_path, n=4, seed=0
        )
        self.assertEqual(total, 5)
        self.assertEqual(n_upi_skipped, 1)
        self.assertEqual(n_unparsable, 0)
        self.assertEqual(len(selected), 4)
        for header, _sequence in selected:
            self.assertFalse(header.is_uniparc_only)

    def test_no_uniparc_only_cluster_is_ever_selected_regardless_of_seed(self):
        # The invariant that actually matters (independent of shuffle order): whatever gets
        # selected never includes a UniParc-only representative.
        for seed in range(10):
            selected, *_ = sample_clusters(self.fasta_path, n=4, seed=seed)
            for header, _sequence in selected:
                self.assertFalse(header.is_uniparc_only)

    def test_sampling_is_reproducible_given_same_seed(self):
        selected_a, *_ = sample_clusters(self.fasta_path, n=3, seed=7)
        selected_b, *_ = sample_clusters(self.fasta_path, n=3, seed=7)
        ids_a = [header.cluster_id for header, _sequence in selected_a]
        ids_b = [header.cluster_id for header, _sequence in selected_b]
        self.assertEqual(ids_a, ids_b)

    def test_requesting_more_than_available_raises(self):
        with self.assertRaises(ValueError):
            sample_clusters(self.fasta_path, n=100, seed=1)


if __name__ == "__main__":
    unittest.main()
