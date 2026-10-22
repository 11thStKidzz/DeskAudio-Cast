import json
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import privacy_scan
ROOT=Path(__file__).resolve().parents[2]

class ReleaseSafetyTests(unittest.TestCase):
    def test_project_context_is_detected_even_after_username_remapping(self):
        fixture=('/local/'+'.chatgpt'+'-projects/'+ 'g-p-'+'a'*32+'/src/lib.rs').encode()
        for body in [fixture,fixture.decode().encode('utf-16le')]:
            self.assertTrue(any(x['type']=='internal_project_path' for x in privacy_scan.scan_bytes(body)))

    def test_redacted_scanner_detects_windows_paths_in_bytes_and_utf16(self):
        fixture="C:"+chr(92)+"Users"+chr(92)+"private_test"+chr(92)+"source.rs"
        for body in [fixture.encode(),fixture.encode("utf-16le")]:
            result=privacy_scan.scan_bytes(body)
            self.assertTrue(any(x["type"]=="personal_windows_path" for x in result))
            self.assertNotIn("private_test",json.dumps(result))

    def test_no_fdk_dependency_or_encoder_implementation(self):
        import tomllib
        lock=tomllib.loads((ROOT/"Cargo.lock").read_text(encoding="utf-8"))
        self.assertFalse(any(p["name"].startswith("fdk-aac") for p in lock["package"]))
        encoder=(ROOT/"vendor/airplay2-gpl3/crates/airplay-audio/src/encoder.rs").read_text(encoding="utf-8")
        self.assertNotIn("fdk_aac::",encoder)
        self.assertNotIn("pub struct AacEncoder",encoder)
        self.assertIn("rejects_aac_without_linking_an_aac_encoder",encoder)

    def test_no_plaintext_persistent_identity_storage(self):
        connection=(ROOT/"vendor/airplay2-gpl3/crates/airplay-client/src/connection.rs").read_text(encoding="utf-8")
        self.assertNotIn("struct PersistentIdentity",connection)
        self.assertNotIn("fs::write",connection)
        self.assertNotIn(".airplay_sender_identity_",connection)
        self.assertIn("Persistent pairing is disabled",connection)
        self.assertIn("pub async fn connect_with_pin(",connection)
        controller=(ROOT/"vendor/airplay2-gpl3/crates/airplay-pairing/src/controller.rs").read_text(encoding="utf-8").split("#[cfg(test)]")[0]
        self.assertNotIn("std::fs::",controller)
        self.assertIn("Plaintext identity storage is disabled",controller)

    def test_removed_audio_dump_callsites(self):
        streamer=(ROOT/"vendor/airplay2-gpl3/crates/airplay-audio/src/streamer.rs").read_text(encoding="utf-8")
        self.assertNotIn("first_4=",streamer)
        self.assertNotIn("first_8=",streamer)
        pairing=(ROOT/"vendor/airplay2-gpl3/crates/airplay-pairing/src/pair_verify.rs").read_text(encoding="utf-8")
        srp=(ROOT/"vendor/airplay2-gpl3/crates/airplay-crypto/src/srp.rs").read_text(encoding="utf-8")
        self.assertNotIn("eprintln!",pairing)
        self.assertNotIn("eprintln!",srp)

    def test_no_automatic_publication_or_artifact_upload(self):
        for file in (ROOT/".github/workflows").glob("*.yml"):
            active="\n".join(line for line in file.read_text(encoding="utf-8").splitlines() if not line.lstrip().startswith("#"))
            self.assertNotIn("upload-artifact@",active)
            self.assertNotIn("tauri-action@",active)
            self.assertNotIn("contents: write",active)
            self.assertNotIn("gh release upload",active)
            self.assertNotIn("  push:",active)
        gates=json.loads((ROOT/"RELEASE_GATES.json").read_text(encoding="utf-8"))
        self.assertIs(gates["public_distribution_authorized"],False)
        self.assertEqual(gates["gates"]["protocol_license_compatibility"],"active_gpl3_source_migration_verified_other_distribution_gates_pending")

if __name__=="__main__":
    unittest.main()
