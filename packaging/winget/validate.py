#!/usr/bin/env python3
"""Validate the winget manifests against the official JSON schemas (needs jsonschema, pyyaml).
Usage: validate.py <schema-dir> [manifest-dir]  (schemas: winget-cli/schemas/JSON/manifests/v1.9.0)"""
import glob, json, sys, yaml, jsonschema
sd = sys.argv[1]
md = sys.argv[2] if len(sys.argv) > 2 else "packaging/winget/manifests"
kinds = {"version": "version", "installer": "installer", "defaultLocale": "defaultLocale"}
bad = 0
for f in sorted(glob.glob(f"{md}/**/*.yaml", recursive=True)):
    doc = yaml.safe_load(open(f, encoding="utf-8"))
    kind = doc["ManifestType"]
    schema = json.load(open(f"{sd}/manifest.{kinds[kind]}.1.9.0.json", encoding="utf-8"))
    errs = list(jsonschema.Draft7Validator(schema).iter_errors(doc))
    for e in errs:
        print(f"{f}: {e.json_path}: {e.message}")
    bad += len(errs)
    print(f"{'FAIL' if errs else 'ok  '} {f}")
sys.exit(1 if bad else 0)
