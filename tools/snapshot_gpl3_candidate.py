"""Local candidate source checkpoint and upstream patch evidence. Never uploads."""
import difflib
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
CANDIDATE = ROOT / "local-validation/gpl3-candidate-20261021"
UPSTREAM = CANDIDATE / "vendor/airplay2-rs"
REVISION = "f824007798f71605307384829800273a4dbbcee2"
HISTORY = ROOT / "local-validation/f1-20261021/upstream.git"

def source_files(root):
    excluded = {"node_modules", "dist", "target", ".git", "gen", "__pycache__"}
    return sorted(p for p in root.rglob("*") if p.is_file()
                  and not (set(p.relative_to(root).parts) & excluded)
                  and p.name != ".DS_Store" and p.suffix not in {".tar", ".log"})

def main():
    origin = json.loads((CANDIDATE / "UPSTREAM_ORIGIN.json").read_text(encoding="utf-8-sig"))
    assert origin["revision"] == REVISION and origin["old_fork_copied"] is False
    expected = {p["path"]: p["sha256"] for p in origin["original_files"]}
    evidence = ROOT / "docs/license-evidence/gpl3-migration"
    evidence.mkdir(parents=True, exist_ok=True)
    diff, matrix = [], []
    for file in source_files(UPSTREAM):
        relative = file.relative_to(UPSTREAM).as_posix()
        current = file.read_bytes()
        if relative in expected:
            original = subprocess.check_output(["git", f"--git-dir={HISTORY}", "show", f"{REVISION}:{relative}"])
            # git archive on this Windows host applied core.autocrlf. Prove the
            # exported baseline is either the blob or its newline-only conversion.
            exported = original if hashlib.sha256(original).hexdigest() == expected[relative] else original.replace(b'\n', b'\r\n')
            assert hashlib.sha256(exported).hexdigest() == expected[relative], relative
        else:
            original = b""
        changed = original.replace(b'\r\n',b'\n') != current.replace(b'\r\n',b'\n')
        matrix.append({"path": relative, "origin": "GPLv3 upstream" if relative in expected else "DeskAudio Cast addition",
                       "modified": changed, "sha256": hashlib.sha256(current).hexdigest()})
        if changed:
            diff.extend(difflib.unified_diff(original.decode("utf-8").replace('\r\n','\n').splitlines(True), current.decode("utf-8").replace('\r\n','\n').splitlines(True),
                        fromfile="a/"+relative if original else "/dev/null", tofile="b/"+relative))
    patch = "".join(diff)
    (evidence / "protocol.patch").write_text(patch, encoding="utf-8", newline="\n")
    files = source_files(CANDIDATE)
    manifest = [{"path":p.relative_to(CANDIDATE).as_posix(), "sha256":hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]
    stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
    archive = ROOT.parent / "private-backups" / f"gpl3-candidate-source-{stamp}.zip"
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as output:
        for file in files:
            output.write(file, file.relative_to(CANDIDATE).as_posix())
    with zipfile.ZipFile(archive) as check:
        assert check.testzip() is None
        for item in manifest:
            assert hashlib.sha256(check.read(item["path"])).hexdigest() == item["sha256"]
    report = {"upstream_repository":origin["repository"], "upstream_revision":REVISION,
              "target_license":"GPL-3.0-or-later", "old_fork_copied":False,
              "protocol_files":matrix, "candidate_source_files":manifest,
              "private_checkpoint":archive.name, "checkpoint_sha256":hashlib.sha256(archive.read_bytes()).hexdigest(),
              "checkpoint_is_public_corresponding_source":False,
              "limitations":["Not a clean-room certification", "Not a complete dependency source bundle",
                             "Not a clean-Windows rebuild or HomePod test", "Public distribution still blocked"]}
    (evidence / "candidate-source.json").write_text(json.dumps(report, ensure_ascii=False, indent=2)+"\n", encoding="utf-8")
    print(json.dumps({"files":len(files),"protocol_modified":sum(p["modified"] for p in matrix),"private_checkpoint":archive.name,"verified":True}))

if __name__ == "__main__":
    main()
