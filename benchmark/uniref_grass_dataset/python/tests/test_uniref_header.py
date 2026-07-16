import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from common.uniref_header import UnparsableHeaderError, parse_header  # noqa: E402


class TestParseHeader(unittest.TestCase):
    def test_standard_header(self):
        line = (
            ">UniRef50_Q9UM73 ALK tyrosine kinase receptor n=3 "
            "Tax=Euarchontoglires TaxID=314146 RepID=ALK_HUMAN"
        )
        header = parse_header(line)
        self.assertEqual(header.cluster_id, "UniRef50_Q9UM73")
        self.assertEqual(header.description, "ALK tyrosine kinase receptor")
        self.assertEqual(header.n_members, 3)
        self.assertEqual(header.tax_name, "Euarchontoglires")
        self.assertEqual(header.taxid, 314146)
        self.assertEqual(header.repid, "ALK_HUMAN")
        self.assertEqual(header.representative_accession, "Q9UM73")
        self.assertFalse(header.is_uniparc_only)

    def test_uniparc_only_representative_is_detected(self):
        line = (
            ">UniRef50_UPI000123ABCD Uncharacterized protein n=1 "
            "Tax=Homo sapiens TaxID=9606 RepID=UPI000123ABCD"
        )
        header = parse_header(line)
        self.assertEqual(header.representative_accession, "UPI000123ABCD")
        self.assertTrue(header.is_uniparc_only)

    def test_missing_taxid_is_tolerated(self):
        line = ">UniRef50_P12345 Some protein n=2 Tax=Mus musculus RepID=SOME_MOUSE"
        header = parse_header(line)
        self.assertEqual(header.description, "Some protein")
        self.assertIsNone(header.taxid)
        self.assertEqual(header.repid, "SOME_MOUSE")

    def test_unparsable_line_raises(self):
        with self.assertRaises(UnparsableHeaderError):
            parse_header(">not_a_uniref_header_at_all")


if __name__ == "__main__":
    unittest.main()
