"""Generate a local Windows-x64 review inventory from actual Cargo metadata.
No downloads. Classifications are candidates, not proof of final binary retention.
License texts are copied verbatim with normalized newlines. Missing texts block release.
"""
import argparse
import hashlib
import json
from pathlib import Path

def collect(metadata, repo):
    supplement_index=repo/'legal/third-party/supplements/index.json'
    supplements=json.loads(supplement_index.read_text(encoding='utf-8'))['supplements'] if supplement_index.is_file() else []
    supplement_map={(p['name'],p['version']):p for p in supplements}
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = next(p["id"] for p in packages.values() if p["name"] == "conexion-airplay")
    roles, seen, pending = {}, set(), [(root, "runtime-candidate")]
    while pending:
        ident, role = pending.pop()
        if (ident, role) in seen:
            continue
        seen.add((ident, role))
        package = packages[ident]
        if any("proc-macro" in t["kind"] for t in package["targets"]):
            role = "build-only"
        roles.setdefault(ident, set()).add(role)
        for dep in nodes.get(ident, {}).get("deps", []):
            for kind in dep["dep_kinds"]:
                if kind["kind"] == "dev":
                    continue
                pending.append((dep["pkg"], "build-only" if kind["kind"] == "build" or role == "build-only" else role))
    entries, texts = [], []
    for ident in sorted(roles, key=lambda i: (packages[i]["name"], packages[i]["version"])):
        p = packages[ident]
        directory = Path(p["manifest_path"]).parent
        files = {f for f in directory.iterdir() if f.is_file() and f.name.upper().startswith(("LICENSE","LICENCE","COPYING","NOTICE"))}
        # Bundled native/third-party subdirectories may have additional notices.
        if (p.get('source') or '').startswith('registry+'):
            files.update(f for f in directory.rglob('*') if f.is_file() and f.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE')))
        if p.get("license_file"):
            candidate = directory / p["license_file"]
            if candidate.is_file():
                files.add(candidate)
        protocol_root=next((root for root in (repo/'vendor/airplay2-gpl3',repo/'vendor/airplay2-rs') if directory.is_relative_to(root)),None)
        protocol = protocol_root is not None
        workspace = directory.is_relative_to(repo) and not protocol
        if protocol:
            files.add(protocol_root / "LICENSE")
        elif workspace:
            files.add(repo / "LICENSE")
        record = {
            "name":p["name"], "version":p["version"], "source":p.get("source") or ("local-modified-protocol" if protocol else "workspace"),
            "repository":p.get("repository"), "declared_license":p.get("license"), "authors":p.get('authors',[]),
            "roles":sorted(roles[ident]), "license_files":[],
            "review_status":"blocked-license-compatibility" if protocol else ("texts-present-not-legally-cleared" if files else "missing-root-text-review-required"),
        }
        if protocol_root and protocol_root.name=='airplay2-gpl3':
            record['review_status']='explicit-gpl3-origin-not-final-distribution-clearance'
            record['source_revision']=json.loads((protocol_root/'UPSTREAM_ORIGIN.json').read_text(encoding='utf-8-sig'))['revision']
        supplement=supplement_map.get((p['name'],p['version']))
        if supplement:
            if supplement['license']!=p.get('license'): raise ValueError('Supplement license declaration mismatch')
            record['supplement_basis']=supplement['basis']
            record['supplement_sources']=supplement['files']
            for item in supplement['files']:
                supplied=repo/'legal/third-party/supplements'/item['file']
                if not supplied.is_file():raise ValueError('Missing supplemental text')
                files.add(supplied)
            if not protocol:
                record['review_status']='supplemented-declared-terms-not-rights-clearance'
        for file in sorted(files):
            body = file.read_text(encoding="utf-8", errors="replace")
            record["license_files"].append({"file":file.relative_to(directory).as_posix() if file.is_relative_to(directory) else file.name,"sha256":hashlib.sha256(file.read_bytes()).hexdigest()})
            texts.append("\n"+"="*72+"\n"+p["name"]+" "+p["version"]+" — "+file.name+"\n"+body.replace("\r\n","\n"))
        if not files:
            # Preserve observed per-file declaration evidence, but never pretend it is full license text.
            evidence = directory / "src" / "lib.rs"
            if evidence.is_file():
                header = "\n".join(evidence.read_text(encoding="utf-8",errors="replace").splitlines()[:12])
                if "License" in header or "license" in header or "Copyright" in header:
                    record["header_evidence"] = header
        entries.append(record)
    return entries, texts

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("metadata",type=Path)
    parser.add_argument("--repo",type=Path,default=Path(__file__).resolve().parents[1])
    args=parser.parse_args()
    raw=args.metadata.read_text(encoding="utf-8-sig")
    # Native cargo warnings can precede the JSON. Never copy manifest absolute paths into public inventory.
    data=json.loads(next(line for line in raw.splitlines() if line.startswith('{"packages":')))
    entries,texts=collect(data,args.repo.resolve())
    npm=json.loads((args.repo/"ui/package-lock.json").read_text(encoding="utf-8"))
    frontend=[]
    for key,p in npm["packages"].items():
        if not key:
            continue
        role="build-only" if p.get("dev") else "runtime-candidate"
        helper_notice=key in {'node_modules/vite','node_modules/esbuild','node_modules/rollup'}
        frontend.append({"name":key.removeprefix("node_modules/"),"version":p["version"],"role":role,
            "generated_helper_notice_included":helper_notice,
            "source":p.get("resolved"),"integrity":p.get("integrity"),"license":p.get("license")})
        if role=="runtime-candidate" or helper_notice:
            folder=args.repo/"ui"/key
            for file in folder.glob("LICENSE*"):
                if file.is_file():
                    texts.append("\n"+"="*72+"\n"+key+" "+p["version"]+" — "+file.name+"\n"+file.read_text(encoding="utf-8",errors="replace"))
    output=args.repo/"legal/third-party"
    output.mkdir(parents=True,exist_ok=True)
    inventory={"schema":1,"target":"x86_64-pc-windows-msvc","status":"license-material-inventory-not-legal-clearance",
        "limitations":["Candidate dependency graph, not final linked-code SBOM","Native Microsoft components require separate terms","Missing root license text is not proof of no license","Proc-macros/build dependencies are separated","No patent clearance"],
        "rust":entries,"frontend":frontend,
        "native":json.loads((args.repo/"legal/native/COMPONENTS.json").read_text(encoding="utf-8"))["components"]}
    (output/"inventory.json").write_text(json.dumps(inventory,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    # Identical license texts are stored once, with every associated component listed.
    # This keeps the in-app document small without dropping copyrights or grant text.
    unique={}
    for segment in texts:
        _,_,component,body=segment.split("\n",3)
        digest=hashlib.sha256(body.encode("utf-8")).hexdigest()
        entry=unique.setdefault(digest,{"components":[],"body":body})
        entry["components"].append(component)
    combined="\n".join("\n"+"="*72+"\n"+"\n".join(entry["components"])+"\n"+entry["body"] for entry in unique.values())
    (output/"LICENSE_TEXTS.txt").write_text("Third-party copyright and license texts / 第三方版权与许可证原文\n"+combined,encoding="utf-8")
    notice=["桌面声渡 / DeskAudio Cast：第三方版权与许可声明","以下列出组件名称、版本和原始授权，完整版权及许可原文附后，并保存在 third-party/LICENSE_TEXTS.txt。","Component names, versions and original license declarations are listed below. Full copyright and license texts follow and are also provided in third-party/LICENSE_TEXTS.txt.","依赖可能经链接优化省略；inventory.json 区分运行候选和构建工具。 / Link optimization may omit components; the inventory distinguishes runtime candidates and build tools.",""]
    for p in entries:
        if "runtime-candidate" in p["roles"]:
            if not p['license_files'] or not p['declared_license']:
                raise ValueError('Missing runtime license evidence: '+p['name'])
            notice.append(f'{p["name"]} {p["version"]}: {p["declared_license"]}')
    notice+=["","Native notices: legal/native/COMPONENTS.json and the original license files beside it.",
        "WebView2 SDK Loader 1.0.3650.58 has its original Microsoft BSD-3-Clause terms; the separate Runtime is not bundled.",
        "MSIX declares the Microsoft VCLibs Desktop framework dependency, supplied separately by Microsoft Store. NSIS requires the system VC++ x64 runtime. Neither runtime is copied into this installer. NSIS 3.11 uses zlib compression; original COPYING is included.",
        "Rust standard library 1.99.1 is partly statically linked, not merely a build tool. Its original copyright/license report and all referenced license texts are provided at legal/native/rust-1.99.1/COPYRIGHT-library.html. Open that installed file for the full offline notices. The full Rust compiler is not distributed as a runtime component.",
        "音频传输采用 ALAC 编码。 / Audio transport uses ALAC encoding.",
        "UI: @tauri-apps/api "+npm["packages"]["node_modules/@tauri-apps/api"]["version"]+" — full cached license included.",
        "Build generators: Vite injects a modulepreload runtime helper. Original Vite, esbuild and Rollup license notices are also retained for generated helpers; this does not claim their entire compilers are shipped at runtime.",
        "应用图标来源说明见对应源码文档。第三方商标仅作兼容性说明。 / Application icon provenance is documented in the corresponding source. Third-party trademarks describe compatibility only."]
    (args.repo/"legal/THIRD_PARTY_NOTICES.txt").write_text("\n".join(notice)+"\n",encoding="utf-8")
    missing=[p["name"] for p in entries if p["review_status"]=="missing-root-text-review-required" and "runtime-candidate" in p["roles"]]
    print(json.dumps({"rust_total":len(entries),"runtime_candidates":sum("runtime-candidate" in p["roles"] for p in entries),
        "frontend_packages":len(frontend),"missing_root_license_texts_runtime":missing,"status":"runtime_texts_present" if not missing else "blocked_missing_runtime_texts"},ensure_ascii=False))

if __name__=="__main__":
    main()
