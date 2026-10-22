import sys
from pathlib import Path
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import f1_license_review as review

class F1LicenseTests(unittest.TestCase):
    def test_dual_license_can_choose_mit(self):
        result=review.gpl2_assessment("MIT OR Apache-2.0")
        self.assertEqual(result["choice"],["MIT"])
        self.assertNotEqual(result["status"],"gpl2-incompatible-expression")

    def test_apache_and_mit_cannot_discard_apache(self):
        self.assertEqual(review.gpl2_assessment("Apache-2.0 AND MIT")["status"],"gpl2-incompatible-expression")
        self.assertEqual(review.gpl2_assessment("Apache-2.0")["status"],"gpl2-incompatible-expression")

    def test_nested_expression_preserves_additional_obligations(self):
        self.assertEqual(review.gpl2_assessment("(Apache-2.0 OR MIT) AND BSD-3-Clause")["choice"],["BSD-3-Clause","MIT"])
        self.assertEqual(review.gpl2_assessment("(MIT OR Apache-2.0) AND Unicode-3.0")["choice"],["MIT","Unicode-3.0"])

    def test_gpl3_or_later_is_not_a_gpl2_option(self):
        self.assertEqual(review.gpl2_assessment("GPL-3.0-or-later")["status"],"gpl2-incompatible-expression")

    def test_mpl_secondary_license_is_conditional_not_blanket_approval(self):
        self.assertEqual(review.gpl2_assessment("MPL-2.0")["status"],"conditional-secondary-license-review")

    def test_missing_or_unhandled_grants_never_pass(self):
        self.assertEqual(review.gpl2_assessment(None)["status"],"grant-unresolved")
        self.assertEqual(review.gpl2_assessment("GPL-2.0-only WITH Some-exception")["status"],"manual-expression-review")
        self.assertEqual(review.gpl2_assessment("MIT @")["status"],"manual-expression-review")

    def test_legacy_dual_license_slash_remains_a_review_choice(self):
        self.assertEqual(review.gpl2_assessment("MIT/Apache-2.0")["choice"],["MIT"])

if __name__=="__main__":
    unittest.main()
