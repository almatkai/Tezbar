"""An absent/invalid HID receipt must never enter a verified-quiet cohort."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('quiet_summary', Path(__file__).with_name('summarize-quiet.py'))
quiet = importlib.util.module_from_spec(spec)
spec.loader.exec_module(quiet)


class QuietReceiptTests(unittest.TestCase):
    def test_valid_quiet_window(self):
        self.assertEqual(quiet.contaminated([
            {'t': 100., 'hid_idle_s': 10.},
            {'t': 130., 'hid_idle_s': 40.},
        ]), [])

    def test_input_anywhere_during_window_is_detected(self):
        self.assertEqual(quiet.contaminated([
            {'t': 100., 'hid_idle_s': 10.},
            {'t': 115., 'hid_idle_s': 0.2},
            {'t': 130., 'hid_idle_s': 15.2},
        ]), [15., 30.])

    def test_missing_or_invalid_receipts_fail_closed(self):
        for value in [None, float('nan'), float('inf'), -1., '20', True]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                quiet.contaminated([{'t': 100., 'hid_idle_s': 10.},
                                    {'t': 130., 'hid_idle_s': value}])
        with self.assertRaises(ValueError):
            quiet.contaminated([{'t': 100.}])

    def test_empty_or_unordered_windows_fail_closed(self):
        for samples in [[], [{'t': 101., 'hid_idle_s': 10.},
                             {'t': 100., 'hid_idle_s': 11.}],
                        [{'t': float('nan'), 'hid_idle_s': 10.}]]:
            with self.subTest(samples=samples), self.assertRaises(ValueError):
                quiet.contaminated(samples)


if __name__ == '__main__':
    unittest.main()
