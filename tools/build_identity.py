"""Canonical product source identity, usable without a Git checkout.
Old protocol backups, user data, generated files and dependency caches are not inputs.
Dependency versions/checksums remain pinned in the root lock files.
"""
import argparse
import hashlib
import os
from pathlib import Path

DIRECTORIES=('crates','src-tauri','ui','legal','tools','packaging','vendor/airplay2-gpl3')
FILES=('Cargo.toml','Cargo.lock','LICENSE','MODIFICATIONS.md')
EXCLUDED={'target','node_modules','dist','gen','__pycache__','.git','.vite'}

def source_paths(root):
    files={root/name for name in FILES if (root/name).is_file()}
    for directory in DIRECTORIES:
        # Do not descend into excluded caches. Filtering only after rglob visits
        # every old protocol target/node_modules entry before rejecting it.
        for current, directories, names in os.walk(root/directory, followlinks=False):
            directories[:] = sorted(d for d in directories if d not in EXCLUDED)
            for name in names:
                path=Path(current)/name
                relative=path.relative_to(root)
                if path.is_file() and not (set(relative.parts)&EXCLUDED) and path.name!='.DS_Store' and not (relative.parts[0]=='vendor' and path.name=='Cargo.lock'):
                    files.add(path)
    return sorted(files,key=lambda path:path.relative_to(root).as_posix())

def source_lines(root):
    return [p.relative_to(root).as_posix()+':'+hashlib.sha256(p.read_bytes()).hexdigest() for p in source_paths(root)]

def fingerprint(lines):
    return hashlib.sha256('\n'.join(lines).encode('utf-8')).hexdigest()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parents[1])
    parser.add_argument('--manifest',type=Path)
    args=parser.parse_args()
    lines=source_lines(args.root.resolve())
    if args.manifest:args.manifest.write_text('\n'.join(lines)+'\n',encoding='utf-8')
    print(fingerprint(lines))

if __name__=='__main__':main()
