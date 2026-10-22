"""Prepare a LOCAL source-material archive. No publication or legal clearance."""
import hashlib
import json
from pathlib import Path
import shutil
import tomllib
import zipfile
import sys
from datetime import datetime, timezone
from build_identity import source_paths, source_lines, fingerprint
import privacy_scan

ROOT=Path(__file__).resolve().parents[1]

def validate_relative(path):
    if path.is_absolute() or '..' in path.parts or set(path.parts)&{'.git','private-backups','local-validation','target','airplay2-rs'}:
        raise ValueError('Forbidden source-package path')

def main():
    if len(sys.argv)==3 and sys.argv[1]=='--resume':
        output=Path(sys.argv[2]).resolve()
        if output.parent != ROOT/'local-source-preparation' or not output.name.startswith('source-package-'):
            raise ValueError('Resume must use a local staging directory')
        manifest=json.loads((output/'SOURCE_PACKAGE_MANIFEST.json').read_text(encoding='utf-8'))
        canonical={p.relative_to(output).as_posix() for p in source_paths(output)}
        # Permit deliberate product fixes only if the ENTIRE product snapshot matches
        # the checkout. Every dependency and supplementary file must remain unchanged.
        for item in manifest['files']:
            if item['path']=='SOURCE_PACKAGE_MANIFEST.json':continue # Repair an interrupted resume's obsolete self-entry.
            actual=hashlib.sha256((output/item['path']).read_bytes()).hexdigest()
            if actual != item['sha256'] and item['path'] not in canonical:
                raise ValueError('Staging dependency/supplement changed')
        if fingerprint(source_lines(ROOT)) != fingerprint(source_lines(output)):
            raise ValueError('Staging product does not match checkout')
        finish(output,manifest['rust_registry_packages'])
        return
    stamp=datetime.now(timezone.utc).strftime('%Y%m%d-%H%M%S')
    output=ROOT/'local-source-preparation'/('source-package-'+stamp)
    output.mkdir(parents=True,exist_ok=False)
    for source in source_paths(ROOT):
        relative=source.relative_to(ROOT);validate_relative(relative)
        if not source.resolve().is_relative_to(ROOT):raise ValueError('External symlink')
        findings=privacy_scan.scan_bytes(source.read_bytes())
        if findings:raise ValueError('Source preflight finding at '+relative.as_posix())
        target=output/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target)
    for name in ['README.md','SOURCE_BUILDING.md','DEPLOYMENT.md','RELEASE_GATES.json',
                 '.github/workflows/ci.yml','.github/workflows/release.yml','docs/图标生成说明.md',
                 'docs/license-evidence/PUBLIC_AUTHORIZATION_SOURCES.json',
                 'docs/license-evidence/F1-fixed-revision-review.json']:
        source=ROOT/name
        if source.is_file():
            if privacy_scan.scan_bytes(source.read_bytes()):raise ValueError('Supplemental source preflight finding at '+name)
            target=output/name;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target)
    # Only packages in the FINAL lock, not stale extras from an earlier vendoring run.
    lock=tomllib.loads((ROOT/'Cargo.lock').read_text(encoding='utf-8'))
    registry=[p for p in lock['package'] if p.get('source','').startswith('registry+')]
    selected=[]
    for p in registry:
        name=f'{p["name"]}-{p["version"]}'
        source=ROOT/'local-source-preparation/rust-vendor'/name
        if not source.is_dir():raise ValueError('Missing locked dependency '+name)
        checksums=json.loads((source/'.cargo-checksum.json').read_text(encoding='utf-8'))
        if checksums['package']!=p['checksum']:raise ValueError('Registry checksum mismatch '+name)
        for relative,expected in checksums['files'].items():
            if hashlib.sha256((source/relative).read_bytes()).hexdigest()!=expected:raise ValueError('Vendored file changed '+name)
        shutil.copytree(source,output/'third-party/rust'/name)
        selected.append({'name':p['name'],'version':p['version'],'registry_checksum':p['checksum'],'license_metadata_and_notices':'preserved in original package'})
    # Existing Windows npm build dependencies; do not copy the user's global cache.
    shutil.copytree(ROOT/'ui/node_modules',output/'ui/node_modules')
    config=output/'.cargo/config.toml';config.parent.mkdir(exist_ok=True)
    config.write_text('[source.crates-io]\nreplace-with="vendored-sources"\n[source.vendored-sources]\ndirectory="third-party/rust"\n',encoding='utf-8')
    npm=json.loads((ROOT/'ui/package-lock.json').read_text(encoding='utf-8'))
    for relative,p in npm['packages'].items():
        if not relative:continue
        package=output/'ui'/relative/'package.json'
        if not package.is_file():
            if p.get('optional'):continue
            raise ValueError('Missing npm package '+relative)
        if json.loads(package.read_text(encoding='utf-8'))['version']!=p['version']:raise ValueError('npm version mismatch')
    finish(output,selected)

def finish(output,selected):
    identity=fingerprint(source_lines(ROOT))
    if fingerprint(source_lines(output))!=identity:raise ValueError('Archive source identity differs')
    manifest=[]
    for file in sorted(output.rglob('*')):
        if file.is_file() and file.name!='SOURCE_PACKAGE_MANIFEST.json':manifest.append({'path':file.relative_to(output).as_posix(),'sha256':hashlib.sha256(file.read_bytes()).hexdigest()})
    package_manifest={'source_fingerprint':identity,'application_baseline':'e93535bcfeedcc3a12e3d11a15e25564208f0d78-d','protocol_revision':'f824007798f71605307384829800273a4dbbcee2','rust_registry_packages':selected,'files':manifest,'public_distribution_authorized':False,'limitations':['Native original terms preserved; patent/trademark/Store review remains independent','Not a clean Windows OS test','Existing npm tree checked by version and snapshot hash, not a fresh registry tarball reinstallation','No public source offer has been made']}
    (output/'SOURCE_PACKAGE_MANIFEST.json').write_text(json.dumps(package_manifest,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    archive=output.with_suffix('.zip')
    version=1
    while archive.exists():
        version+=1;archive=output.with_suffix(f'.v{version}.zip')
    partial=output.with_suffix('.zip.partial')
    attempt=0
    while partial.exists():
        attempt+=1;partial=output.with_suffix(f'.zip.retry{attempt}.partial')
    print('Snapshot complete; creating archive',flush=True)
    # crates.io reproducible sources can have 1970 mtimes. ZIP starts at 1980;
    # clamp metadata only, never alter file bytes or dependency checksums.
    with zipfile.ZipFile(partial,'x',zipfile.ZIP_DEFLATED,compresslevel=1,strict_timestamps=False) as zipped:
        for file in sorted(output.rglob('*')):
            if file.is_file():zipped.write(file,file.relative_to(output).as_posix())
    with zipfile.ZipFile(partial) as zipped:
        assert zipped.testzip() is None
    partial.rename(archive) # An interrupted write must never look like a completed ZIP.
    report={'directory':output.name,'archive':archive.name,'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'files':len(manifest),'registry_packages':len(selected),'source_fingerprint':identity,'old_fork_included':False,'clean_build':'not_yet_run','public_distribution_authorized':False}
    evidence=ROOT/'docs/license-evidence/source-delivery';evidence.mkdir(parents=True,exist_ok=True)
    (evidence/'LATEST.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report))

if __name__=='__main__':main()
