"""Checks for user-facing freeze materials; not a legal-clearance test."""
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]

class ReleaseFreezeTests(unittest.TestCase):
    def test_user_documents_have_no_obsolete_work_in_progress_notices(self):
        files = ['legal/ABOUT.txt','legal/PRIVACY.txt','legal/OPEN_SOURCE.txt',
                 'legal/THIRD_PARTY_NOTICES.txt','legal/SUPPORT.txt',
                 'legal/STORE_EULA_DRAFT.txt','packaging/windows/使用与安装说明.txt']
        for name in files:
            text = (ROOT/name).read_text(encoding='utf-8')
            for obsolete in ['尚未提交审核','将在提交前配置','核查中，禁止','remains pending',
                             'not yet configured','store_candidate_not_submitted','（本地测试版）']:
                self.assertNotIn(obsolete, text, name)
        privacy = (ROOT/'legal/PRIVACY.txt').read_text(encoding='utf-8')
        self.assertNotIn('0.1.9', privacy)

    def test_both_complete_privacy_languages_are_embedded_without_summary_substitution(self):
        body = (ROOT/'legal/PRIVACY.txt').read_text(encoding='utf-8')
        for name in ['PRIVACY.zh-Hans.txt','PRIVACY.en-US.txt']:
            text = (ROOT/'legal'/name).read_text(encoding='utf-8').strip()
            self.assertIn(text, body)
            for section in range(1,9): self.assertIn(f'\n{section}. ', '\n'+text)
        self.assertNotIn('English summary', body)

    def test_msix_declares_desktop_crt_without_bundled_downloader(self):
        root = ET.parse(ROOT/'packaging/windows/AppxManifest.xml').getroot()
        ns = {'f':'http://schemas.microsoft.com/appx/manifest/foundation/windows10'}
        dependency = root.find('f:Dependencies/f:PackageDependency',ns)
        self.assertEqual(dependency.attrib['Name'],'Microsoft.VCLibs.140.00.UWPDesktop')
        self.assertEqual(dependency.attrib['MinVersion'],'14.0.33728.0')
        self.assertEqual(dependency.attrib['Publisher'],'CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US')
        code = (ROOT/'src-tauri/src/runtime_prerequisites.rs').read_text(encoding='utf-8')
        self.assertIn('RegGetValueW',code)
        self.assertIn('MessageBoxW',code)
        for forbidden in ['Command::new','reqwest','URLDownloadToFile','RegSetValue']:
            self.assertNotIn(forbidden,code)

    def test_source_links_have_the_explicitly_approved_repository(self):
        text = (ROOT/'legal/OPEN_SOURCE.txt').read_text(encoding='utf-8')
        self.assertIn('https://github.com/HotRiver7/DeskAudio-Cast/releases/tag/v0.1.10',text)
        self.assertIn('DeskAudioCast-0.1.10-corresponding-source.zip',text)
        self.assertIn('收费再分发',text)
        self.assertIn('redistribution for a fee',text)

if __name__ == '__main__': unittest.main()
