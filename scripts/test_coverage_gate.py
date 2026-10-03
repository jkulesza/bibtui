import unittest
from check_coverage import check


class CoverageGateTests(unittest.TestCase):
    def report(self, covered):
        summary = {"lines": {"count": 100, "covered": covered}}
        return {"data": [{"totals": summary, "files": [
            {"filename": "C:\\repo\\src\\app\\mod.rs", "summary": summary}
        ]}]}

    def test_current_coverage_passes_and_regressions_fail(self):
        baseline = {"TOTAL": 80, "src/app/mod.rs": 80}
        self.assertEqual(check(self.report(80), baseline), [])
        self.assertEqual(len(check(self.report(79), baseline)), 2)

    def test_missing_module_cannot_silently_pass(self):
        failures = check(self.report(100), {"src/missing.rs": 50})
        self.assertEqual(failures, ["src/missing.rs: missing from coverage report"])
