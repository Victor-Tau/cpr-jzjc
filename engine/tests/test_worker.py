import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "engine"))
import worker


class WorkerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bank = json.loads((ROOT / "vendor/modeltrace/data/unified_bank.json").read_text())

    def test_current_bank_and_all_calibrations(self):
        r = worker.validate(self.bank)
        self.assertEqual(len(r["models"]), len(self.bank["models"]))

    def test_rejects_changed_method_and_invalid_dimensions(self):
        cases = []
        b = copy.deepcopy(self.bank); b["method"]["range"] = [1, 999]; cases.append(b)
        b = copy.deepcopy(self.bank); b["robust"]["hellinger"]["centroids"].pop(); cases.append(b)
        b = copy.deepcopy(self.bank); b["robust"]["model_order"].reverse(); cases.append(b)
        b = copy.deepcopy(self.bank); b["calibration"]["3"]["beta"] = float("nan"); cases.append(b)
        b = copy.deepcopy(self.bank); b["models"][1]["id"] = b["models"][0]["id"]; cases.append(b)
        b = copy.deepcopy(self.bank); b["robust"]["ordered_blocks"]["weight"] = 0.5; cases.append(b)
        for b in cases:
            with self.subTest():
                with self.assertRaises(ValueError): worker.validate(b)

    def test_sample_refusal_and_truncation(self):
        for text in ["Sorry I cannot help", "1,2,3", "1,2,3 " * 2000]:
            with self.assertRaises(ValueError): worker.handle({"op":"sample","text":text,"expectedCount":310})

    def test_single_sample_score_matches_upstream(self):
        numbers = [((i*i + i*97) % 355) + 1 for i in range(310)]
        samples = [{"numbers":numbers, "expectedCount":310}]
        got = worker.handle({"op":"score", "path":str(ROOT / "vendor/modeltrace/data/unified_bank.json"), "samples":samples})
        expected = worker.fp.analyze_global_outputs([{"text":",".join(map(str,numbers)),"expected_count":310}],self.bank)
        self.assertEqual(got["prediction"], expected["prediction"])
        self.assertEqual([r["probability"] for r in got["results"]], [r["probability"] for r in expected["results"]])
        with self.assertRaises(ValueError):
            worker.handle({"op":"score", "path":str(ROOT / "vendor/modeltrace/data/unified_bank.json"), "samples":samples * 3})

    def test_challenges_are_one_valid_job(self):
        a = worker.handle({"op":"challenges"})
        self.assertEqual(len(a), 1)
        self.assertEqual(len({x["id"] for x in a}), 1)
        self.assertTrue(all(292 <= x["expected_count"] <= 332 for x in a))

    def test_cached_bank_refreshes_when_file_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bank.json"
            path.write_text('{"version":1}')
            first = worker.load(path)
            self.assertEqual(first, {"version":1})
            self.assertIs(worker.load(path), first)
            modified = path.stat().st_mtime_ns + 1_000_000
            path.write_text('{"version":2}')
            os.utime(path, ns=(modified, modified))
            self.assertEqual(worker.load(path), {"version":2})
            self.assertIsNot(worker.load(path), first)


if __name__ == "__main__":
    unittest.main()
