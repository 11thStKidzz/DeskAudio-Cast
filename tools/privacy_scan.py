"""Local privacy/FDK marker scan. Never prints matched secret values.
Not a substitute for manual secret review, symbol/link-map inspection or legal clearance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

SOURCE_EXT={".rs",".ts",".js",".mjs",".json",".toml",".md",".txt",".yml",".yaml",".html",".css",".ps1",".cmd",".py"}
EXCLUDED={".git","target","node_modules","dist","local-validation","local-source-preparation","__pycache__"}
PATTERNS={
    "internal_project_path":re.compile(rb"\.chatgpt-projects[\\/]|g-p-[0-9a-f]{32}",re.I),
    "personal_windows_path":re.compile(rb"[A-Za-z]:[\\/]+Users[\\/]+[A-Za-z0-9_. -]+",re.I),
    "personal_unix_path":re.compile(rb"/(?:Users|home)/[A-Za-z0-9_.-]+/"),
    "private_key_block":re.compile(rb"-----BEGIN (?:RSA |EC |OPENSSH |ENCRYPTED )?PRIVATE KEY-----"),
    "github_token":re.compile(rb"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{50,})"),
    "aws_access_key":re.compile(rb"\bAKIA[A-Z0-9]{16}\b"),
}
FDK_MARKERS=[b"aacEncOpen",b"aacEncEncode",b"aacEncoder_SetParam",b"FDKaacEnc",b"Fraunhofer FDK"]

def scan_bytes(data, patterns=PATTERNS):
    found=[]
    for encoding,body in [("bytes",data),("utf16le",data.decode("utf-16le",errors="ignore").encode("utf-8"))]:
        for kind,pattern in patterns.items():
            matches=list(pattern.finditer(body))
            if matches:
                found.append({"type":kind,"encoding":encoding,"count":len(matches),
                    "lines":sorted({body[:m.start()].count(b"\n")+1 for m in matches})[:20] if encoding=="bytes" else []})
    return found

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--repo",type=Path,default=Path(__file__).resolve().parents[1])
    parser.add_argument("--binary",type=Path,action="append",default=[])
    parser.add_argument("--output",type=Path)
    args=parser.parse_args()
    root=args.repo.resolve()
    report={"scope":"current-source-and-explicit-binaries","limitations":["Pattern scan only; no universal secret or patent clearance","Ignored files and old binaries not sanitized","Public upstream attribution is retained"],"source":[],"binaries":[]}
    for file in root.rglob("*"):
        rel=file.relative_to(root)
        if set(rel.parts)&EXCLUDED or not file.is_file() or file.suffix not in SOURCE_EXT:
            continue
        findings=scan_bytes(file.read_bytes())
        if findings:
            report["source"].append({"path":rel.as_posix(),"findings":findings})
    for file in args.binary:
        data=file.read_bytes()
        report["binaries"].append({"file":file.name,"bytes":len(data),"sha256":hashlib.sha256(data).hexdigest(),
            "findings":scan_bytes(data),"fdk_markers":[m.decode() for m in FDK_MARKERS if m.lower() in data.lower()]})
    result=json.dumps(report,ensure_ascii=False,indent=2)+"\n"
    if args.output:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(result,encoding="utf-8")
    print(json.dumps({"source_flagged_files":len(report["source"]),"binaries":report["binaries"]},ensure_ascii=False))

if __name__=="__main__":
    main()
