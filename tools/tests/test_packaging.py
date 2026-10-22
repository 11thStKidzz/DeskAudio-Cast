import hashlib
import json
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET
ROOT=Path(__file__).resolve().parents[2]

class PackagingTests(unittest.TestCase):
    def test_application_and_package_versions_agree(self):
        import tomllib
        version=json.loads((ROOT/'src-tauri/tauri.conf.json').read_text(encoding='utf-8'))['version']
        self.assertEqual(tomllib.loads((ROOT/'Cargo.toml').read_text(encoding='utf-8'))['workspace']['package']['version'],version)
        for file in ['package.json','package-lock.json']:
            self.assertEqual(json.loads((ROOT/'ui'/file).read_text(encoding='utf-8'))['version'],version)
        manifest=ET.parse(ROOT/'packaging/windows/AppxManifest.xml').getroot()
        store_version=manifest.find('{http://schemas.microsoft.com/appx/manifest/foundation/windows10}Identity').attrib['Version']
        # Store package numbering is independent of the pre-1.0 application version.
        self.assertEqual(store_version,'1.0.'+version.split('.')[-1]+'.0')
        parts=[int(p) for p in store_version.split('.')]
        self.assertTrue(0 < parts[0] <= 65535)
        self.assertTrue(all(0 <= p <= 65535 for p in parts))
        self.assertEqual(parts[-1],0)

    def test_tray_popup_uses_only_local_content_and_existing_event_permissions(self):
        text=(ROOT/'src-tauri/src/tray_panel.rs').read_text(encoding='utf-8')
        self.assertIn('WebviewUrl::App("tray.html".into())',text)
        self.assertNotIn('WebviewUrl::External',text)
        capability=json.loads((ROOT/'src-tauri/capabilities/default.json').read_text(encoding='utf-8'))
        self.assertEqual(set(capability['windows']),{'main','tray-panel'})
        self.assertEqual(set(capability['permissions']),{'core:event:allow-listen','core:event:allow-unlisten'})
        lib=(ROOT/'src-tauri/src/lib.rs').read_text(encoding='utf-8')
        tray=lib.split('async fn tray_toggle_stream',1)[1].split('fn toggle_main_window',1)[0]
        self.assertIn('_=token.cancelled()',tray)
        self.assertIn('Some(token)',tray)

    def test_all_26_license_supplements_match_their_recorded_public_blobs(self):
        base=ROOT/'legal/third-party/supplements'
        data=json.loads((base/'index.json').read_text(encoding='utf-8'))['supplements']
        self.assertEqual(len(data),26)
        for entry in data:
            for file in entry['files']:
                body=(base/file['file']).read_bytes()
                git_blob=hashlib.sha1(('blob '+str(len(body))+'\0').encode()+body).hexdigest()
                self.assertEqual(git_blob,file['git_blob_sha'],entry['name'])

    def test_native_original_license_hashes(self):
        data=json.loads((ROOT/'legal/native/COMPONENTS.json').read_text(encoding='utf-8'))
        for component in data['components']:
            if 'license_file' in component:
                body=(ROOT/'legal/native'/component['license_file']).read_bytes()
                self.assertEqual(hashlib.sha256(body).hexdigest(),component['license_sha256'])
        self.assertEqual(data['components'][0]['license'],'BSD-3-Clause')

    def test_installer_does_not_elevate_or_delete_user_data(self):
        text=(ROOT/'packaging/windows/installer.nsi').read_text(encoding='utf-8')
        self.assertIn('RequestExecutionLevel user',text)
        self.assertIn('SetCompressor /SOLID zlib',text)
        for forbidden in ['RMDir /r','taskkill','netsh','URLDownloadToFile','WriteRegStr HKLM']:
            self.assertNotIn(forbidden,text)
        self.assertEqual(text.count('ExecWait'),1)
        self.assertIn('ExecWait \'"$PLUGINSDIR\\vc_redist.x64.exe" /install /norestart',text)
        self.assertIn('MB_DEFBUTTON2',text)
        self.assertIn('Call VCRuntimeReady',text)
        self.assertIn('IfErrors in_use',text)
        self.assertIn('ReadRegStr $0 HKCU',text)

    def test_msix_manifest_matches_partner_center_with_opt_in_startup(self):
        root=ET.parse(ROOT/'packaging/windows/AppxManifest.xml').getroot()
        ns={'f':'http://schemas.microsoft.com/appx/manifest/foundation/windows10','d':'http://schemas.microsoft.com/appx/manifest/desktop/windows10'}
        identity=root.find('f:Identity',ns).attrib
        self.assertEqual(identity['Name'],'11thStKidzz.DeskAudioCast')
        self.assertEqual(identity['Publisher'],'CN=F8685FFB-73E3-4962-83BD-CEE132D1ADF6')
        self.assertEqual(root.find('f:Properties/f:PublisherDisplayName',ns).text,'11thStKidzz')
        self.assertEqual(root.find('f:Properties/f:DisplayName',ns).text,'ms-resource:AppDisplayName')
        task=root.find('.//d:StartupTask',ns)
        self.assertEqual(task.attrib['TaskId'],'DeskAudioCastStartup')
        self.assertEqual(task.attrib['Enabled'],'false')
        self.assertNotIn('broadFileSystemAccess',ET.tostring(root).decode())

    def test_store_names_are_localized_separately_from_identity(self):
        for language,name in [('en-US','DeskAudio Cast'),('zh-Hans','桌面声渡')]:
            resource=ET.parse(ROOT/'packaging/windows/Strings'/language/'Resources.resw').getroot()
            data={d.attrib['name']:d.find('value').text for d in resource.findall('data')}
            self.assertEqual(data['AppDisplayName'],name)
        script=(ROOT/'tools/package_windows.ps1').read_text(encoding='utf-8')
        self.assertIn('resources.pri',script)
        self.assertIn('/dq en-US',script)
        self.assertNotIn('Import-Certificate',script)

    def test_product_code_retains_credential_and_log_boundaries(self):
        lib=(ROOT/'src-tauri/src/lib.rs').read_text(encoding='utf-8')
        self.assertIn('log_retention::prune',lib)
        text=(ROOT/'src-tauri/src/log_retention.rs').read_text(encoding='utf-8')
        self.assertIn('.share_mode(0)',text)
        self.assertIn('is_symlink()',text)
        self.assertNotIn('remove_dir_all',text.split('#[cfg(test)]')[0])

    def test_generated_frontend_helpers_have_original_notices(self):
        inventory=json.loads((ROOT/'legal/third-party/inventory.json').read_text(encoding='utf-8'))
        selected={p['name'] for p in inventory['frontend'] if p.get('generated_helper_notice_included')}
        self.assertEqual(selected,{'vite','esbuild','rollup'})
        text=(ROOT/'legal/third-party/LICENSE_TEXTS.txt').read_text(encoding='utf-8')
        for name in selected:self.assertIn('node_modules/'+name+' ',text)

    def test_rust_standard_library_original_notices_are_self_contained(self):
        import re
        base=ROOT/'legal/native'
        data=json.loads((base/'COMPONENTS.json').read_text(encoding='utf-8'))
        entry=next(c for c in data['components'] if c['name']=='Rust standard library')
        original=base/entry['license_file']
        self.assertEqual(entry['version'],'1.99.1 (58cf8f9c3 2026-10-13)')
        self.assertEqual(hashlib.sha256(original.read_bytes()).hexdigest(),entry['license_sha256'])
        html=original.read_text(encoding='utf-8')
        # This toolchain embeds license bodies inline; it need not link a licenses/ file.
        # Validate any relative links if a later pinned distribution uses them.
        self.assertIn('Copyright notices for The Rust Standard Library',html)
        self.assertIn('Permission is hereby granted',html)
        self.assertIn('Apache License',html)
        linked={p.split('#')[0] for p in re.findall(r'href="(licenses/[^"]+)"',html)}
        for relative in linked:
            self.assertTrue((original.parent/relative).is_file(),relative)
        for item in entry['additional_license_files']:
            self.assertEqual(hashlib.sha256((base/item['file']).read_bytes()).hexdigest(),item['sha256'])
        self.assertTrue(entry['standard_library_artifacts'])

if __name__=='__main__':unittest.main()
