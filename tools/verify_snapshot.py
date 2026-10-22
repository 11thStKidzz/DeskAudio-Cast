"""Verify extracted bytes, source identity and confinement without slow shell-per-file calls."""
import argparse
import hashlib
import json
from pathlib import Path
from build_identity import source_lines, fingerprint

def verify(root):
    root=root.resolve()
    manifest=json.loads((root/'SOURCE_PACKAGE_MANIFEST.json').read_text(encoding='utf-8'))
    seen=set()
    for item in manifest['files']:
        relative=Path(item['path'])
        if relative.is_absolute() or '..' in relative.parts or relative.as_posix()=='SOURCE_PACKAGE_MANIFEST.json':
            raise ValueError('Invalid manifest member')
        path=(root/relative).resolve()
        if not path.is_relative_to(root) or item['path'] in seen:raise ValueError('Path escape or duplicate')
        seen.add(item['path'])
        if hashlib.sha256(path.read_bytes()).hexdigest()!=item['sha256']:raise ValueError('Snapshot hash mismatch: '+item['path'])
    if fingerprint(source_lines(root))!=manifest['source_fingerprint']:raise ValueError('Product source fingerprint mismatch')
    return len(seen)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--root',type=Path,required=True);args=parser.parse_args()
    print(json.dumps({'verified_snapshot_files':verify(args.root),'git_required':False}))
