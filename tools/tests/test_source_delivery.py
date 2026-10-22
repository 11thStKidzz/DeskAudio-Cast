import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import build_identity
from prepare_source_package import validate_relative

ROOT=Path(__file__).resolve().parents[2]

class SourceDeliveryTests(unittest.TestCase):
    def test_identity_prunes_excluded_build_trees_before_scanning_files(self):
        visited=[]
        original=__import__('os').walk
        def walk(*args,**kwargs):
            for current,dirs,names in original(*args,**kwargs):
                visited.append(Path(current).name)
                yield current,dirs,names
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            for relative in ['ui/node_modules/deep/cache.js','vendor/airplay2-gpl3/target/huge/cache.bin','ui/src/app.ts']:
                path=root/relative;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('fixture')
            with patch.object(build_identity.os,'walk',side_effect=walk):
                selected=build_identity.source_paths(root)
            self.assertEqual([p.relative_to(root).as_posix() for p in selected],['ui/src/app.ts'])
            self.assertNotIn('node_modules',visited)
            self.assertNotIn('target',visited)

    def test_old_fork_history_outputs_and_credentials_paths_are_not_allowed(self):
        for path in ['vendor/airplay2-rs/src/lib.rs','.git/config','target/debug/app.exe','../settings.json','local-validation/log.json','private-backups/key']:
            with self.assertRaises(ValueError):validate_relative(Path(path))
        validate_relative(Path('vendor/airplay2-gpl3/src/lib.rs'))

    def test_identity_has_no_git_or_personal_cache_dependency(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            for path in ['Cargo.toml','crates/core/lib.rs','vendor/airplay2-gpl3/lib.rs','vendor/airplay2-rs/old.rs','ui/node_modules/cache.js','src-tauri/gen/schema.json','vendor/airplay2-gpl3/Cargo.lock']:
                file=root/path;file.parent.mkdir(parents=True,exist_ok=True);file.write_text('fixture',encoding='utf-8')
            selected={p.relative_to(root).as_posix() for p in build_identity.source_paths(root)}
            self.assertEqual(selected,{'Cargo.toml','crates/core/lib.rs','vendor/airplay2-gpl3/lib.rs'})
            first=build_identity.fingerprint(build_identity.source_lines(root))
            (root/'vendor/airplay2-rs/old.rs').write_text('private backup changed',encoding='utf-8')
            self.assertEqual(first,build_identity.fingerprint(build_identity.source_lines(root)))
            (root/'crates/core/lib.rs').write_text('code changed',encoding='utf-8')
            self.assertNotEqual(first,build_identity.fingerprint(build_identity.source_lines(root)))

    def test_active_manifests_do_not_reference_old_fork(self):
        self.assertNotIn('../../vendor/airplay2-rs/',(ROOT/'crates/airplay-core/Cargo.toml').read_text(encoding='utf-8'))
        inventory=json.loads((ROOT/'legal/third-party/inventory.json').read_text(encoding='utf-8'))
        protocol=[p for p in inventory['rust'] if p['source']=='local-modified-protocol']
        self.assertEqual(len(protocol),9)
        self.assertTrue(all(p['declared_license']=='GPL-3.0-or-later' for p in protocol))
        self.assertTrue(all(p['license_files'] for p in inventory['rust'] if 'runtime-candidate' in p['roles']))

    def test_webview_has_no_shell_plugin(self):
        self.assertNotIn('tauri-plugin-shell',(ROOT/'src-tauri/Cargo.toml').read_text(encoding='utf-8'))
        code=(ROOT/'src-tauri/src/lib.rs').read_text(encoding='utf-8')
        self.assertNotIn('app.shell()',code)
        self.assertIn('system.join("explorer.exe")',code)

if __name__=='__main__':unittest.main()
