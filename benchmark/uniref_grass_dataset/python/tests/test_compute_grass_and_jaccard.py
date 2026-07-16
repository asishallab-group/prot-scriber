import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from compute_grass_and_jaccard import (  # noqa: E402
    geometric_mean4,
    grass_score,
    jaccard_similarity,
    overlap,
    strip_organism_suffix,
    tokenize,
)


class TestOverlap(unittest.TestCase):
    def test_full_length_alignment_on_both_sides_is_one(self):
        # both sequences fully covered by the alignment (length 100 each)
        self.assertAlmostEqual(overlap(1, 100, 100, 1, 100, 100), 1.0)

    def test_partial_alignment(self):
        # a: fully covered (1-100 of length 100); b: half covered (1-50 of length 100)
        # overlap = (100 + 50) / (100 + 100) = 0.75
        self.assertAlmostEqual(overlap(1, 100, 100, 1, 50, 100), 0.75)


class TestGrassScore(unittest.TestCase):
    def test_geometric_mean_of_perfect_scores_is_one(self):
        self.assertAlmostEqual(geometric_mean4(1.0, 1.0, 1.0, 1.0), 1.0)

    def test_grass_score_matches_hand_computed_value(self):
        expected = (0.8 * 0.9 * 0.7 * 0.95) ** 0.25
        self.assertAlmostEqual(grass_score(0.8, 0.9, 0.7, 0.95), expected)

    def test_any_zero_term_zeroes_the_score(self):
        self.assertEqual(grass_score(0.0, 0.9, 0.7, 0.95), 0.0)


class TestStripOrganismSuffix(unittest.TestCase):
    def test_strips_trailing_bracketed_organism(self):
        self.assertEqual(
            strip_organism_suffix("alcohol dehydrogenase [Homo sapiens]"),
            "alcohol dehydrogenase",
        )

    def test_leaves_description_without_suffix_unchanged(self):
        self.assertEqual(strip_organism_suffix("alcohol dehydrogenase"), "alcohol dehydrogenase")


class TestTokenize(unittest.TestCase):
    def test_splits_on_non_alnum_and_lowercases(self):
        self.assertEqual(
            tokenize("Alcohol-Dehydrogenase, C-terminal"),
            {"alcohol", "dehydrogenase", "c", "terminal"},
        )


class TestJaccardSimilarity(unittest.TestCase):
    def test_identical_descriptions_are_1(self):
        self.assertEqual(
            jaccard_similarity("alcohol dehydrogenase", "alcohol dehydrogenase"), 1.0
        )

    def test_completely_disjoint_descriptions_are_0(self):
        self.assertEqual(
            jaccard_similarity("alcohol dehydrogenase", "ribosomal protein"), 0.0
        )

    def test_case_is_ignored(self):
        self.assertEqual(
            jaccard_similarity("Alcohol Dehydrogenase", "ALCOHOL DEHYDROGENASE"), 1.0
        )

    def test_partial_overlap(self):
        # {"alcohol","dehydrogenase"} vs {"alcohol","oxidase"} -> intersection=1, union=3
        self.assertAlmostEqual(
            jaccard_similarity("alcohol dehydrogenase", "alcohol oxidase"), 1 / 3
        )

    def test_refseq_organism_suffix_is_stripped_from_hit_only(self):
        # Without stripping, "homo"/"sapiens" would count against the hit; with
        # stripping, the two descriptions are equivalent apart from case.
        query_description = "alcohol dehydrogenase"
        hit_description = "alcohol dehydrogenase [Homo sapiens]"
        self.assertEqual(jaccard_similarity(query_description, hit_description), 1.0)

    def test_both_descriptions_empty_is_nan(self):
        self.assertTrue(math.isnan(jaccard_similarity("", "")))


if __name__ == "__main__":
    unittest.main()
