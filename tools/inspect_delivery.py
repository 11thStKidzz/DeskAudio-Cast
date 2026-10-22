"""Read actual archives/payloads. Redact findings, never dump matching values."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import zipfile
import privacy_scan

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--archive',type=Path)
    parser.add_argument('--payload',type=Path)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    report={'user_path_findings':[],'pattern_findings':[],'forbidden_entries':[],
            'limitations':['Pattern checks are not universal secret/legal clearance',
                          'Public upstream test keys/paths must be distinguished from user credentials']}
    # Do not print the actual home or user-specific tokens in the report.
    home=str(Path.home())
    private=[home.encode(),home.replace('\\','/').encode(),home.encode('utf-16le')]
    count=0
    def inspect(name,body):
        nonlocal count
        count+=1
        path=PurePosixPath(name)
        build_output=bool(path.parts) and (path.parts[0] in {'target','private-backups','local-validation'} or path.parts[:2] in {('src-tauri','target'),('ui','dist')})
        # A compiler's src/target/*.rs (e.g. cc) is required source, not build output.
        if '..' in path.parts or path.is_absolute() or build_output or any(p in path.parts for p in ['.git','airplay2-rs']) or path.suffix.lower() in ['.pdb','.log','.dmp','.wav','.pcm','.alac','.mp3','.m4a']:
            report['forbidden_entries'].append(name)
        if any(value.lower() in body.lower() for value in private):report['user_path_findings'].append(name)
        found=privacy_scan.scan_bytes(body)
        if found:report['pattern_findings'].append({'path':name,'findings':found,'scope':'third-party-original' if name.startswith('third-party/') or '/node_modules/' in name else 'product'})
    if args.archive:
        with zipfile.ZipFile(args.archive) as archive:
            report['crc_bad_member']=archive.testzip()
            names=archive.namelist()
            manifest=json.loads(archive.read('SOURCE_PACKAGE_MANIFEST.json')) if 'SOURCE_PACKAGE_MANIFEST.json' in names else None
            expected={p['path']:p['sha256'] for p in manifest['files']} if manifest else {}
            bad=[]
            if 'SOURCE_PACKAGE_MANIFEST.json' in expected:bad.append('manifest_must_not_hash_itself')
            for item in archive.infolist():
                if item.is_dir():continue
                body=archive.read(item)
                inspect(item.filename,body)
                if manifest and item.filename!='SOURCE_PACKAGE_MANIFEST.json' and hashlib.sha256(body).hexdigest()!=expected.get(item.filename):bad.append(item.filename)
            if manifest:
                bad+=sorted(set(expected)-set(names))
                report['manifest_hash_mismatches']=bad
                report['source_fingerprint']=manifest['source_fingerprint']
            report['archive_sha256']=hashlib.sha256(args.archive.read_bytes()).hexdigest()
    if args.payload:
        for file in args.payload.rglob('*'):
            if file.is_file():inspect(file.relative_to(args.payload).as_posix(),file.read_bytes())
    report['files_inspected']=count
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'files':count,'private_home_matches':len(report['user_path_findings']),'patterns':len(report['pattern_findings']),'forbidden_entries':len(report['forbidden_entries']),'manifest_mismatches':len(report.get('manifest_hash_mismatches',[]))}))
    if report['user_path_findings'] or report['forbidden_entries'] or report.get('manifest_hash_mismatches'):raise SystemExit(1)
if __name__=='__main__':main()
