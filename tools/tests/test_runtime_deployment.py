"""Deployment contract, source links and installer error paths (not OS installation)."""
import json,re,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]

class RuntimeDeploymentTests(unittest.TestCase):
    def test_actual_tauri_and_custom_packager_share_fixed_layout(self):
        conf=json.loads((ROOT/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))
        self.assertEqual(conf['bundle']['windows']['webviewInstallMode'],{'type':'skip'})
        lock=json.loads((ROOT/'packaging/windows/runtime-lock.json').read_text(encoding='utf-8'))
        code=(ROOT/'src-tauri/src/runtime_prerequisites.rs').read_text(encoding='utf-8')
        self.assertIn(lock['webview2']['version'],code)
        self.assertIn('"WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",dir.join(FIXED_RUNTIME_FOLDER)',code)
        self.assertIn('std::env::current_exe()',code)
        pack=(ROOT/'tools/package_windows.ps1').read_text(encoding='utf-8')
        self.assertIn("Copy-Item -LiteralPath $runtimeDirectory -Destination (Join-Path $payload 'WebView2Fixed') -Recurse",pack)
        self.assertIn('fetch_windows_runtimes.ps1',pack)
        self.assertNotIn('vc_redist.x64.exe',pack.split("Copy-Item -LiteralPath $exe",1)[1].split('$inventory=',1)[0])

    def test_official_materials_pinned_and_not_silently_installed(self):
        lock=json.loads((ROOT/'packaging/windows/runtime-lock.json').read_text(encoding='utf-8'))
        for item in lock.values():self.assertRegex(item['sha256'],r'^[0-9a-f]{64}$')
        code=(ROOT/'tools/fetch_windows_runtimes.ps1').read_text(encoding='utf-8')
        self.assertIn('Get-AuthenticodeSignature',code)
        self.assertIn('Microsoft Corporation',code)
        self.assertIn('if(-not $Download)',code)
        for bad in ['Start-Process','/quiet','Import-Certificate']:self.assertNotIn(bad,code)
        manifest=json.loads((ROOT/'packaging/windows/webview2-files.sha256.json').read_text(encoding='utf-8'))
        self.assertEqual(manifest['version'],lock['webview2']['version'])
        self.assertEqual(manifest['source_cab_sha256'],lock['webview2']['sha256'])
        self.assertEqual(len({f['path'] for f in manifest['files']}),len(manifest['files']))

    def test_installer_consent_version_recheck_abort_and_windows10_acl(self):
        code=(ROOT/'packaging/windows/installer.nsi').read_text(encoding='utf-8')
        for text in ['MB_DEFBUTTON2','IfSilent vc_cancel','Call VCRuntimeReady','${VC_MIN_VERSION}',
                     'ExecWait','Call EnsureVCRuntime','SetErrorLevel 1603','SetErrorLevel 1602',
                     '$2 != 3010','$2 != 1638','*S-1-15-2-2:(OI)(CI)(RX)','*S-1-15-2-1:(OI)(CI)(RX)']:
            self.assertIn(text,code)
        self.assertEqual(code.count('Call VCRuntimeReady'),2)
        for bad in ['EdgeUpdate','/quiet','/passive','Stop-Process','taskkill']:self.assertNotIn(bad,code)

    def test_all_user_legal_entries_use_current_owner_without_changing_upstream(self):
        for directory in ['legal','packaging','ui/src','src-tauri/src']:
            for p in (ROOT/directory).rglob('*'):
                if not p.is_file() or p.suffix not in {'.txt','.md','.rs','.ts','.json','.html','.nsi','.ps1'}:continue
                text=p.read_text(encoding='utf-8')
                self.assertNotRegex(text,r'(?i)https?://(?:github\.com|raw\.githubusercontent\.com)/HotRiver7/DeskAudio-Cast',str(p.relative_to(ROOT)))
        for name in ['ABOUT.txt','PRIVACY.txt','OPEN_SOURCE.txt','SUPPORT.txt','STORE_EULA_DRAFT.txt']:
            text=(ROOT/'legal'/name).read_text(encoding='utf-8')
            for url in re.findall(r'https://(?:github\.com|raw\.githubusercontent\.com)/[^\s]+',text):
                if 'DeskAudio-Cast' in url:self.assertIn('/11thStKidzz/DeskAudio-Cast',url)
        self.assertIn('lmcgartland/airplay2-rs',(ROOT/'legal/OPEN_SOURCE.txt').read_text(encoding='utf-8'))

    def test_microsoft_runtime_is_not_relicensed_or_hidden_and_smartscreen_notice_present(self):
        code=(ROOT/'src-tauri/src/legal.rs').read_text(encoding='utf-8')
        self.assertIn('WebView2-Fixed-Runtime-EULA.txt',code)
        for name in ['PRIVACY.txt','PRIVACY.zh-Hans.txt','PRIVACY.en-US.txt']:
            text=(ROOT/'legal'/name).read_text(encoding='utf-8')
            self.assertIn('Microsoft Defender SmartScreen',text)
            self.assertIn('privacy-whitepaper#smartscreen',text)
            self.assertIn('deployment-error.txt',text)
        self.assertIn('Microsoft WebView2 Fixed Runtime',(ROOT/'packaging/windows/installer.nsi').read_text(encoding='utf-8'))

if __name__=='__main__':unittest.main()
