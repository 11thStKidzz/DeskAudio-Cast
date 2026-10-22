"""Regression checks for renamed repository and r3 lightweight deployment."""
import json,re,sys,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools'))
from build_identity import source_paths

class RepositoryLinkTests(unittest.TestCase):
    def test_no_retired_owner_project_urls_in_current_product(self):
        owner='Hot'+'River7'
        pattern=re.compile(r'https?://(?:(?:www\.)?(?:github\.com|raw\.githubusercontent\.com|api\.github\.com/repos|uploads\.github\.com/repos)/'+owner+r'/DeskAudio-Cast|'+owner+r'\.github\.io/DeskAudio-Cast)',re.I)
        for p in source_paths(ROOT)+[ROOT/'README.md',ROOT/'SOURCE_BUILDING.md']:
            self.assertIsNone(pattern.search(p.read_text(encoding='utf-8',errors='ignore')),p.relative_to(ROOT))

    def test_english_chinese_privacy_reference_same_live_repository(self):
        for name in ['PRIVACY.txt','PRIVACY.en-US.txt','PRIVACY.zh-Hans.txt','SUPPORT.txt']:
            self.assertIn('https://github.com/11thStKidzz/DeskAudio-Cast',(ROOT/'legal'/name).read_text(encoding='utf-8'))

    def test_privacy_versions_are_current_metadata_not_stale(self):
        version=json.loads((ROOT/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))['version']
        for name in ['PRIVACY.txt','PRIVACY.en-US.txt','PRIVACY.zh-Hans.txt']:
            text=(ROOT/'legal'/name).read_text(encoding='utf-8')
            self.assertIn(version,text)
            self.assertNotIn('0.1.10',text)

    def test_system_runtime_deployment_is_kept_not_fixed_runtime(self):
        script=(ROOT/'tools/package_windows.ps1').read_text(encoding='utf-8')
        for banned in ['fetch_windows_runtimes','WebView2Fixed','vc_redist.x64.exe']:
            self.assertNotIn(banned,script)
        code=(ROOT/'src-tauri/src/runtime_prerequisites.rs').read_text(encoding='utf-8')
        self.assertIn('RegGetValueW',code)
        self.assertNotIn('WEBVIEW2_BROWSER_EXECUTABLE_FOLDER',code)

if __name__=='__main__':unittest.main()
