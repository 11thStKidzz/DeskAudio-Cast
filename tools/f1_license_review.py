"""Read-only Git/source analysis; writes only generated review evidence.
Never changes LICENSE, dependency selection, production code, or Git refs/history.
SPDX expression choices here are conditional review aids, not relicensing grants.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib

PIN="dc66e2b3df1ad87a34fd9c859ec452bd787154e1"
GPL3="714d885a6024cf5b65fdcc6424b5d7bbad828037"
CANDIDATE="f824007798f71605307384829800273a4dbbcee2"
COMMON="a7f019fe6246ebd9701201a8a5c31e9a15243956"
AIR_GRANT="d309942c83248de60e7133b69bf2bb552dff511f"
CRATES={"airplay-"+n for n in ["audio","client","core","crypto","discovery","pairing","resampler","rtsp","timing"]}
SOURCE_SUFFIX={".rs",".toml"}

def git(directory,*args,alternate=None):
    env=os.environ.copy()
    if alternate:
        env["GIT_ALTERNATE_OBJECT_DIRECTORIES"]=str(alternate/"objects")
    result=subprocess.run(["git",f"--git-dir={directory}",*args],check=True,capture_output=True,env=env)
    return result.stdout.decode("utf-8",errors="strict")

def tree(directory,revision):
    out={}
    for line in git(directory,"ls-tree","-r",revision,"--","crates").splitlines():
        meta,path=line.split("\t",1)
        if len(Path(path).parts)>=3 and Path(path).parts[1] in CRATES:
            out[path]=meta.split()[2]
    return out

def expression_options(expression):
    """Small SPDX AND/OR expression reader. Legacy slash is recorded as OR.
    Unknown operators/identifiers remain unapproved; no MIT substring heuristics.
    """
    expression=expression.replace("/"," OR ")
    pattern=r"\(|\)|\bAND\b|\bOR\b|[A-Za-z0-9.+:-]+"
    if re.sub(pattern,"",expression).strip():
        raise ValueError("unsupported SPDX characters")
    tokens=re.findall(pattern,expression)
    index=0
    def factor():
        nonlocal index
        if index>=len(tokens):
            raise ValueError("missing term")
        token=tokens[index]
        index+=1
        if token=="(":
            result=disjunction()
            if index>=len(tokens) or tokens[index]!=")":
                raise ValueError("unbalanced SPDX")
            index+=1
            return result
        if token in {"AND","OR",")","WITH"}:
            raise ValueError("unsupported SPDX construct")
        return [frozenset([token])]
    def conjunction():
        nonlocal index
        result=factor()
        while index<len(tokens) and tokens[index]=="AND":
            index+=1
            right=factor()
            result=[a|b for a in result for b in right]
        return result
    def disjunction():
        nonlocal index
        result=conjunction()
        while index<len(tokens) and tokens[index]=="OR":
            index+=1
            result+=conjunction()
        return result
    result=disjunction()
    if index!=len(tokens):
        raise ValueError("unsupported SPDX expression")
    return result

def gpl2_assessment(expression):
    if not expression:
        return {"status":"grant-unresolved","choice":None}
    try:
        options=expression_options(expression)
    except ValueError:
        return {"status":"manual-expression-review","choice":None}
    incompatible={"Apache-2.0","GPL-3.0-only","GPL-3.0-or-later","GPL-3.0","LGPL-3.0-only","LGPL-3.0-or-later"}
    possible=[o for o in options if not o&incompatible]
    if not possible:
        return {"status":"gpl2-incompatible-expression","choice":None}
    candidate=min(possible,key=lambda o:(len(o),sorted(o)))
    return {"status":"conditional-secondary-license-review" if "MPL-2.0" in candidate else "candidate-only-text-and-native-review-required",
            "choice":sorted(candidate)}

def review(repo,evidence):
    upstream=evidence/"upstream.git"
    fork=evidence/"fork.git"
    air=evidence/"airsend.git"
    for directory in [upstream,fork,air]:
        if not directory.is_dir():
            raise ValueError("Public history mirror missing; no automatic download.")
    base=git(upstream,"merge-base",GPL3,PIN,alternate=fork).strip()
    if base!=COMMON:
        raise ValueError("Unexpected ancestry; stop instead of guessing.")
    old=tree(fork,PIN)
    new=tree(upstream,CANDIDATE)
    comparison=[]
    for path,blob in sorted(old.items()):
        category="same-blob-in-explicit-gpl3-snapshot" if new.get(path)==blob else ("not-in-gpl3-snapshot" if path not in new else "different-content-requires-grant-or-reimplementation")
        comparison.append({"path":path,"fork_blob":blob,"gpl3_blob":new.get(path),"result":category})
    fork_commits=[]
    for line in git(fork,"log",f"{COMMON}..{PIN}","--no-merges","--format=%H%x09%aI%x09%an%x09%s").splitlines():
        sha,date,author,subject=line.split("\t",3)
        in_gpl3=subprocess.run(["git",f"--git-dir={upstream}","merge-base","--is-ancestor",sha,"master"],capture_output=True,
            env={**os.environ,"GIT_ALTERNATE_OBJECT_DIRECTORIES":str(fork/"objects")}).returncode==0
        paths=git(fork,"diff-tree","--no-commit-id","--name-only","-r",sha).splitlines()
        fork_commits.append({"sha":sha,"author":author,"author_date":date,"subject":subject,
            "reachable_from_current_gpl3_upstream":in_gpl3,"paths":paths,
            "scope":"Publication under GPL3 must be evidenced separately; reachability is not alone a rights ruling."})
    changes=[]
    for line in git(fork,"diff","--numstat",COMMON,PIN,"--","crates").splitlines():
        add,delete,path=line.split("\t",2)
        changes.append({"path":path,"added_lines":int(add),"deleted_lines":int(delete)})
    protocol_grants={
        "pinned":tomllib.loads(git(fork,"show",f"{PIN}:Cargo.toml"))["workspace"]["package"]["license"],
        "upstream_gpl3":tomllib.loads(git(upstream,"show",f"{GPL3}:Cargo.toml"))["workspace"]["package"]["license"],
        "application_current":tomllib.loads((repo/"Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["license"],
    }
    inventory=json.loads((repo/"legal/third-party/inventory.json").read_text(encoding="utf-8"))
    deps=[]
    cache=Path(os.environ["USERPROFILE"])/".cargo/registry/src"
    registries=list(cache.glob("*"))
    for package in inventory["rust"]:
        if "runtime-candidate" not in package["roles"]:
            continue
        name,version=package["name"],package["version"]
        item={"name":name,"version":version,"expression":package["declared_license"],"gpl2":gpl2_assessment(package["declared_license"])}
        folder=next((r/f"{name}-{version}" for r in registries if (r/f"{name}-{version}").is_dir()),None)
        if folder and package["declared_license"]=="MPL-2.0":
            optouts=[]
            headers=[]
            for file in folder.rglob("*"):
                if not file.is_file() or file.suffix not in {".rs",".md",".txt"} or file.name.upper().startswith(("LICENSE","LICENCE","COPYING")):
                    continue
                body=file.read_text(encoding="utf-8",errors="replace")
                if "Incompatible With Secondary Licenses" in body:
                    optouts.append(file.relative_to(folder).as_posix())
                if file.name=="lib.rs":
                    lines=body.splitlines()[:12]
                    if any("Mozilla Public" in line for line in lines):
                        headers.append({"file":file.relative_to(folder).as_posix(),"header":"\n".join(lines)})
            item["mpl_review"]={"non_template_optout_matches":optouts,"grant_headers":headers,
                "limitation":"No match is not a complete historical secondary-license clearance; preserve MPL and source duties."}
        deps.append(item)
    target_map=repo/"target/remediation/release/desk-audio-link.map"
    map_text=target_map.read_text(encoding="utf-8",errors="replace") if target_map.is_file() else ""
    report={
        "schema":1,"checked_date":"2026-10-21","scope":"fixed public Git histories plus current Windows runtime-candidate dependency graph",
        "pinned_protocol":PIN,"gpl3_grant_commit":GPL3,"gpl3_upstream_snapshot":CANDIDATE,"common_ancestor":base,
        "application_gpl3_grant_commit":AIR_GRANT,"declared_grants":protocol_grants,
        "heads":{"upstream":git(upstream,"rev-parse","master").strip(),"fork":git(fork,"rev-parse","master").strip(),
                 "application":git(air,"rev-parse","main").strip()},
        "license_document_sha256":{
            "old_protocol":hashlib.sha256(git(fork,"show",f"{PIN}:LICENSE").encode()).hexdigest(),
            "gpl3_protocol":hashlib.sha256(git(upstream,"show",f"{GPL3}:LICENSE").encode()).hexdigest()},
        "comparison_counts":dict(Counter(x["result"] for x in comparison)),
        "file_comparison":comparison,"fork_commits":fork_commits,"fork_changes":changes,
        "fork_change_totals":{"files":len(changes),"added":sum(x["added_lines"] for x in changes),"deleted":sum(x["deleted_lines"] for x in changes)},
        "gpl2_dependency_assessment":deps,
        "linked_symbol_evidence":{"map_sha256":hashlib.sha256(target_map.read_bytes()).hexdigest() if target_map.is_file() else None,
            "spin_sleep_symbol_mentions":len(re.findall("spin_sleep",map_text)),
            "tao_symbol_mentions":len(re.findall("tao",map_text)),
            "dpi_symbol_mentions":len(re.findall(r"\bdpi\b|3dpi",map_text)),
            "limitation":"Mentions are supportive only; target graph and source usage are also reviewed."},
        "decision":{"target_license":"GPL-3.0-or-later","production_protocol_relicensed":False,"F1_resolved":False,
            "blocker":"GPL3 rights for fork-specific additions are not explicitly established by the reviewed evidence.",
            "gpl2_fallback":"not_viable_for_current_combination_without_replacements_and_reworking_upstream_GPL3_application_changes",
            "recommended_no_contact_route":"use explicit GPL3 same-upstream snapshot and independently implement required Windows/live-buffer additions; approval before core migration"},
        "limitations":["Repository commits and declared authorship are evidence, not a judicial ownership determination",
            "No later license was retroactively applied to an earlier commit","No binaries, dependencies or source were published",
            "No contact with maintainers","No production code, third-party licenses or Git history changed by this tool"]
    }
    return report

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--repo",type=Path,default=Path(__file__).resolve().parents[1])
    parser.add_argument("--evidence",type=Path)
    parser.add_argument("--output",type=Path)
    args=parser.parse_args()
    repo=args.repo.resolve()
    evidence=args.evidence or repo/"local-validation/f1-20261021"
    report=review(repo,evidence)
    output=args.output or repo/"docs/license-evidence/F1-fixed-revision-review.json"
    output.parent.mkdir(parents=True,exist_ok=True)
    output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
    print(json.dumps({"comparisons":report["comparison_counts"],"changes":report["fork_change_totals"],
        "gpl2_blockers":[f'{p["name"]} {p["version"]}' for p in report["gpl2_dependency_assessment"] if p["gpl2"]["status"]=="gpl2-incompatible-expression"],
        "linked_symbols":report["linked_symbol_evidence"],"decision":report["decision"]},ensure_ascii=False))

if __name__=="__main__":
    main()
