"""Generated fixtures only: passing these tests is NOT a HomePod measurement."""
from array import array
import json
from pathlib import Path
import random
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import latency_lab as lab


class LatencyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'private-recording.wav'

    def fixture(self, delay=125, missing=None, noise=False, inverted=False):
        rate = 16000
        reference = lab.probe_samples(rate)
        receiver = array('h', [0]) * len(reference)
        shift = delay * rate // 1000
        for i, value in enumerate(reference):
            if missing is not None and lab.ONSETS_MS[missing] <= i*1000/rate < lab.ONSETS_MS[missing]+25:
                continue
            if i + shift < len(receiver):
                receiver[i + shift] = round(value * (-0.4 if inverted else 0.4))
        rng = random.Random(42)
        samples = array('h')
        for a, b in zip(reference, receiver):
            samples.extend((a, b + (rng.randint(-8, 8) if noise else 0)))
        lab.write_pcm(self.path, samples, rate, 2)

    def test_known_125_ms(self):
        self.fixture()
        result = lab.analyze(self.path, kind='synthetic-validation')
        self.assertAlmostEqual(result['median_ms'], 125, delta=2)
        self.assertEqual(result['status'], 'complete')
        self.assertFalse(result['absolute_latency_verified'])

    def test_known_500_ms_with_noise_inversion_and_gain(self):
        self.fixture(500, noise=True, inverted=True)
        self.assertAlmostEqual(lab.analyze(self.path)['median_ms'], 500, delta=2)

    def test_missing_pulse_not_mispaired(self):
        self.fixture(missing=3)
        result = lab.analyze(self.path)
        self.assertEqual(result['status'], 'incomplete')
        self.assertEqual(result['paired_pulses'], 7)
        self.assertEqual(result['missing_receiver'], 1)
        self.assertAlmostEqual(result['median_ms'], 125, delta=2)

    def test_reversed_channels_rejected(self):
        self.fixture()
        with self.assertRaises(ValueError):
            lab.analyze(self.path, 1, 0)

    def test_same_channel_rejected(self):
        self.fixture()
        with self.assertRaises(ValueError):
            lab.analyze(self.path, 0, 0)

    def test_silence_rejected(self):
        lab.write_pcm(self.path, array('h', [0]) * (16000*15*2), 16000, 2)
        with self.assertRaises(ValueError):
            lab.analyze(self.path)

    def test_mono_rejected(self):
        lab.write_pcm(self.path, lab.probe_samples(16000), 16000, 1)
        with self.assertRaises(ValueError):
            lab.analyze(self.path)

    def test_no_overwrite(self):
        self.fixture()
        before = self.path.read_bytes()
        with self.assertRaises(FileExistsError):
            lab.write_pcm(self.path, array('h', [0]), 16000, 1)
        self.assertEqual(before, self.path.read_bytes())

    def test_no_private_audio_or_filename_in_report(self):
        self.fixture()
        report = json.dumps(lab.analyze(self.path))
        self.assertNotIn(self.path.name, report)
        self.assertNotIn('samples', report)

    def test_wrong_pattern_rejected(self):
        with self.assertRaises(ValueError):
            lab.align_pattern([1000, 2000, 3000, 4000, 5000])

    def test_jitter_summary(self):
        # Different delay at each burst, within the declared matching tolerance.
        self.fixture()
        original = lab.read_envelopes
        env0 = [0]*15000
        env1 = [0]*15000
        for onset, delay in zip(lab.ONSETS_MS, [120, 121, 122, 123, 124, 125, 126, 127]):
            env0[onset:onset+20] = [1000]*20
            env1[onset+delay:onset+delay+20] = [1000]*20
        try:
            lab.read_envelopes = lambda *args: ([env0, env1], 1)
            result = lab.analyze(self.path)
        finally:
            lab.read_envelopes = original
        self.assertEqual(result['spread_ms'], 7)
        self.assertEqual(result['p95_ms'], 127)

    def test_clipping_rejected(self):
        samples = array('h', [0]) * (16000*15*2)
        samples[100] = 32767
        lab.write_pcm(self.path, samples, 16000, 2)
        with self.assertRaisesRegex(ValueError, '削波'):
            lab.analyze(self.path)


if __name__ == '__main__':
    unittest.main()
