"""Record full-file changes against the preserved upstream manifest. No Git writes."""
import hashlib
import json
from datetime import date
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def main():
    protocol=ROOT/'vendor/airplay2-gpl3'
    origin=json.loads((protocol/'UPSTREAM_ORIGIN.json').read_text(encoding='utf-8-sig'))
    originals={p['path']:p['sha256'] for p in origin['original_files']}
    records=[]
    for file in sorted(protocol.rglob('*')):
        if not file.is_file() or file.name in {'LOCAL_CHANGES.json','Cargo.lock','UPSTREAM_ORIGIN.json'}:continue
        relative=file.relative_to(protocol).as_posix()
        body=file.read_bytes();digest=hashlib.sha256(body).hexdigest()
        original=originals.get(relative)
        if digest != original:
            records.append({'path':relative,'original_sha256':original,'snapshot_sha256':digest,'status':'modified' if original else 'added'})
    result={'upstream':origin['repository'],'revision':origin['revision'],'grant':origin['license'],
            'date':date.today().isoformat(),'old_fork_copied':False,'records':records,
            'notes':['Full corresponding modified files are supplied, not just patches',
                     'Raw file hashes include line endings; metadata/generated records and the unused upstream Cargo.lock are excluded',
                     'No change to the upstream copyright grant; not a legal clearance']}
    (protocol/'LOCAL_CHANGES.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'records':len(records),'revision':origin['revision']}))
if __name__=='__main__':main()
