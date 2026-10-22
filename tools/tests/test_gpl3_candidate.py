"""Facts about the isolated migration candidate; not a legal-clearance test."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib
import unittest

ROOT=Path(__file__).resolve().parents[2]
CANDIDATE=ROOT/'local-validation/gpl3-candidate-20261021'
PROTOCOL=CANDIDATE/'vendor/airplay2-rs'
REVISION='f824007798f71605307384829800273a4dbbcee2'

class Gpl3CandidateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not CANDIDATE.is_dir():
            raise unittest.SkipTest('Isolated candidate not prepared; no migration verification performed')

    def test_original_license_and_readme_match_pinned_gpl3_source(self):
        origin=json.loads((CANDIDATE/'UPSTREAM_ORIGIN.json').read_text(encoding='utf-8-sig'))
        self.assertEqual(origin['revision'],REVISION)
        self.assertIs(origin['old_fork_copied'],False)
        for name in ['LICENSE','README.md']:
            expected=subprocess.check_output(['git',f'--git-dir={ROOT / "local-validation/f1-20261021/upstream.git"}','show',f'{REVISION}:{name}'])
            self.assertEqual((PROTOCOL/name).read_text(encoding='utf-8'),expected.decode('utf-8').replace('\r\n','\n'))

    def test_actual_protocol_inventory_uses_gpl3_not_the_old_fork(self):
        inventory=json.loads((CANDIDATE/'legal/third-party/inventory.json').read_text(encoding='utf-8'))
        entries=[p for p in inventory['rust'] if p['source']=='local-modified-protocol']
        self.assertEqual({p['name'] for p in entries},{'airplay-audio','airplay-client','airplay-core','airplay-crypto','airplay-discovery','airplay-pairing','airplay-resampler','airplay-rtsp','airplay-timing'})
        self.assertTrue(all(p['declared_license']=='GPL-3.0-or-later' for p in entries))
        self.assertTrue(all(p['repository']=='https://github.com/lmcgartland/airplay2-rs' for p in entries))

    def test_active_lock_and_encoder_do_not_include_fdk(self):
        lock=tomllib.loads((CANDIDATE/'Cargo.lock').read_text(encoding='utf-8'))
        self.assertFalse(any(p['name'].startswith('fdk-aac') for p in lock['package']))
        encoder=(PROTOCOL/'crates/airplay-audio/src/encoder.rs').read_text(encoding='utf-8')
        self.assertNotIn('fdk_aac::',encoder)
        self.assertNotIn('pub struct AacEncoder',encoder)

    def test_alac_production_encoder_is_unchanged_from_gpl3_upstream(self):
        path='crates/airplay-audio/src/encoder.rs'
        original=subprocess.check_output(['git',f'--git-dir={ROOT / "local-validation/f1-20261021/upstream.git"}','show',f'{REVISION}:{path}']).decode('utf-8')
        current=(PROTOCOL/path).read_text(encoding='utf-8')
        start='/// ALAC encoder for realtime streaming.'
        self.assertEqual(original.split(start,1)[1].split('/// AAC encoder',1)[0].strip(),
                         current.split(start,1)[1].split('/// Create appropriate encoder',1)[0].strip())

    def test_plaintext_identity_storage_remains_removed(self):
        connection=(PROTOCOL/'crates/airplay-client/src/connection.rs').read_text(encoding='utf-8')
        self.assertNotIn('struct PersistentIdentity',connection)
        self.assertNotIn('fs::write',connection)
        self.assertNotIn('.airplay_sender_identity_',connection)
        self.assertIn('Persistent pairing is disabled',connection)
        controller=(PROTOCOL/'crates/airplay-pairing/src/controller.rs').read_text(encoding='utf-8').split('#[cfg(test)]')[0]
        self.assertNotIn('std::fs::',controller)

    def test_protocol_log_arguments_are_static_not_audio_or_keys(self):
        count=0
        for file in (PROTOCOL/'crates').glob('*/src/**/*.rs'):
            text=file.read_text(encoding='utf-8')
            self.assertNotIn('eprintln!(',text,file.name)
            self.assertNotIn('dbg!(',text,file.name)
            for args in re.findall(r'\b(?:tracing::)?(?:trace|debug|info|warn|error)!\((.*?)\)',text,re.S):
                count+=1
                self.assertRegex(args.strip(),r'^(?:event\s*=\s*"[^"\n]*",\s*)?"[A-Za-z0-9_]+"$',file.name)
        self.assertGreater(count,200)

    def test_candidate_settings_and_startup_do_not_share_production_identity(self):
        config=json.loads((CANDIDATE/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
        production=json.loads((ROOT/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
        self.assertNotEqual(config['identifier'],production['identifier'])
        self.assertIn('DeskAudioCastGpl3Candidate',(CANDIDATE/'src-tauri/src/startup.rs').read_text(encoding='utf-8'))

    def test_previous_release_stays_intact(self):
        original=ROOT/'target/remediation/release/conexion-airplay.exe'
        self.assertEqual(hashlib.sha256(original.read_bytes()).hexdigest(),'c30cce2f591f325826afb79efb144edda87d9cbc874cf2e71e21d11fb79b6a89')

if __name__=='__main__':
    unittest.main()
