"""Collect license texts for the locked Rust dependency graph before release."""
import json
from pathlib import Path
import subprocess
import sys


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python tools/collect_licenses.py <output-file>")
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], text=True, encoding="utf-8"))
    sections = ["Third-party Rust dependency notices\nGenerated from Cargo.lock.\n"]
    missing = []
    for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
        if package["source"] is None:
            continue
        root = Path(package["manifest_path"]).parent
        candidates = {p for p in root.iterdir() if p.is_file() and
                      (p.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")))}
        if package.get("license_file"):
            candidates.add(root / package["license_file"])
        candidates = sorted(p for p in candidates if p.is_file())
        # This published proc-macro crate omitted its workspace license files.
        # Its MIT option is copied from its exact upstream source commit:
        # rusticata/asn1-rs@a20e5f7319c896737ad0f2557037817b91ad854f/LICENSE-MIT
        if not candidates and (package["name"], package["version"]) == ("asn1-rs-impl", "0.2.0"):
            candidates = [Path(__file__).resolve().parent.parent / "licenses" /
                          "asn1-rs-impl-0.2.0-LICENSE-MIT.txt"]
        if not candidates:
            missing.append(f'{package["name"]} {package["version"]} ({package.get("license")})')
            continue
        sections.append(f'\n{"=" * 72}\n{package["name"]} {package["version"]} | {package.get("license")}\n')
        for path in candidates:
            sections.append(f'\n--- {path.name} ---\n{path.read_text(encoding="utf-8")}\n')
    if missing:
        raise SystemExit("License text missing; inspect before distribution:\n" + "\n".join(missing))
    Path(sys.argv[1]).write_text("".join(sections), encoding="utf-8")
    print(f"License notices generated for {len(metadata['packages']) - 1} dependency packages")


if __name__ == "__main__":
    main()
