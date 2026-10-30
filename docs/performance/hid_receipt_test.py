"""Timed HID receipts: boundary skew, pre-window guard and fail-closed checks."""
import importlib.util
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).parent))
import hid_receipt as h

spec = importlib.util.spec_from_file_location('quiet_summary', Path(__file__).with_name('summarize-quiet.py'))
quiet = importlib.util.module_from_spec(spec)
spec.loader.exec_module(quiet)


def timed(t, idle, latency=.0001, delay=.01):
    return {'t': t, 'hid': {'pre': t+delay, 'post': t+delay+latency, 'idle_s': idle}}


class TimedReceiptTests(unittest.TestCase):
    def test_quiet_window_with_long_prior_quiet(self):
        c = h.classify([timed(100., 50.), timed(115., 65.), timed(130., 80.)])
        self.assertEqual(c['verdict'], 'quiet')
        self.assertAlmostEqual(c['margin_s'], 50.-.0101, places=6)

    def test_input_inside_guard_band_is_not_quiet(self):
        # Last input 1 s before the window: legacy rule calls this quiet; the guard does not.
        samples = [timed(100., 1.01), timed(130., 31.01)]
        self.assertEqual(h.classify(samples)['verdict'], 'pre_window_input')
        self.assertEqual(quiet.contaminated([{'t': s['t'], 'hid_idle_s': s['hid']['idle_s']} for s in samples]), [])

    def test_slow_read_after_window_start_input_is_detected(self):
        # Read happens 0.2 s after `t`; input 0.01 s after the window start.
        # The pass-1 `t + 0.05` rule misses it; the post-read bound does not.
        s = [timed(100., 50.), {'t': 110., 'hid': {'pre': 110.15, 'post': 110.2, 'idle_s': 10.19}}]
        self.assertEqual(h.classify(s)['verdict'], 'in_window_input')
        self.assertEqual(quiet.contaminated([{'t': x['t'], 'hid_idle_s': x['hid']['idle_s']} for x in s]), [])

    def test_last_receipt_covers_input_after_last_cpu_read(self):
        s = [timed(100., 50.), {'t': 130., 'hid': {'pre': 130.01, 'post': 130.02, 'idle_s': .005}}]
        self.assertEqual(h.classify(s)['verdict'], 'in_window_input')

    def test_legacy_receipts_keep_pass1_rule_at_zero_guard(self):
        legacy = [{'t': 100., 'hid_idle_s': 10.}, {'t': 115., 'hid_idle_s': .2}, {'t': 130., 'hid_idle_s': 15.2}]
        self.assertEqual(h.classify(legacy, guard_s=0)['verdict'], 'in_window_input')
        self.assertTrue(quiet.contaminated(legacy))
        self.assertEqual(h.classify([{'t': 100., 'hid_idle_s': 700.}, {'t': 130., 'hid_idle_s': 730.}])['verdict'], 'quiet')

    def test_invalid_receipts_fail_closed(self):
        bad = [
            [],
            [{'t': 100.}],
            [{'t': 100., 'hid': None}],
            [{'t': 100., 'hid': {'pre': 100.1, 'post': 100.05, 'idle_s': 5.}}],   # post before pre
            [{'t': 100., 'hid': {'pre': 99.9, 'post': 100., 'idle_s': 5.}}],      # read before sample start
            [{'t': 100., 'hid': {'pre': 100.1, 'post': 100.2, 'idle_s': None}}],  # unreadable property
            [{'t': 100., 'hid': {'pre': 100.1, 'post': 100.2, 'idle_s': float('nan')}}],
            [timed(101., 5.), timed(100., 5.)],                                    # unordered
            [{'t': True, 'hid_idle_s': 5.}],
        ]
        for samples in bad:
            with self.subTest(samples=samples), self.assertRaises(ValueError):
                h.classify(samples)

    def test_live_receipt_is_timed_and_finite(self):
        import time
        origin = time.monotonic()
        r = h.receipt(origin)
        self.assertLessEqual(r['pre'], r['post'])
        self.assertIsNotNone(r['idle_s'])
        self.assertGreaterEqual(r['idle_s'], 0)


if __name__ == '__main__':
    unittest.main()


class SamplerCallsiteTests(unittest.TestCase):
    def test_receipts_sampler_produces_classifiable_samples(self):
        import time
        spec = importlib.util.spec_from_file_location('receipts', Path(__file__).with_name('post-search-receipts.py'))
        receipts = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(receipts)

        class Fake:
            pid = None
            def __init__(self): self.started, self.samples = time.monotonic(), []
            def sample(self, label):
                s = dict(t=time.monotonic()-self.started, phase=label, processes=[]); self.samples.append(s); return s
        d = receipts.instrument(Fake())
        for _ in range(3): d.sample('post_search_idle')
        for s in d.samples:
            self.assertLessEqual(s['t'], s['hid']['pre'])
            self.assertLessEqual(s['hid']['pre'], s['hid']['post'])
        self.assertIn(h.classify(d.samples)['verdict'], ['quiet', 'pre_window_input', 'in_window_input'])
