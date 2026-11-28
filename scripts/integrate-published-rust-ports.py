"""Verify public dual-target releases and optionally promote matching catalog pins.

Uses already downloaded archives. Does not download, run binaries, publish releases,
or change existing policy. The production importer remains a separate required check.
Only artifacts and Loved nativePort release metadata are updated. Unrelated JSON
objects and their formatting are preserved byte-for-byte.
"""
import argparse
import hashlib
import json
import os
import stat
from pathlib import Path
import tempfile
from urllib.parse import urlsplit
import zipfile

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {"aarch64-apple-darwin", "x86_64-pc-windows-msvc"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def policy(manifest):
    return {key: value for key, value in manifest.items() if key not in ("files", "artifacts")}


def verify(manifest, directory, expected_targets=TARGETS):
    require(set(manifest["artifacts"]) == expected_targets, "Required release targets missing or unexpected")
    require(manifest["schemaVersion"] == 3 and manifest["runtime"] == "rust"
            and manifest["execution"] == "process" and manifest["protocolVersion"] == 1,
            "Expected schema-3 Rust process")
    for target, pin in manifest["artifacts"].items():
        url = urlsplit(pin["url"])
        require(url.scheme == "https" and url.hostname == "github.com"
                and not url.username and not url.password and not url.query,
                "Expected credential-free public GitHub release URL")
        archive = directory / url.path.rsplit("/", 1)[-1]
        require(archive.stat().st_size <= 128 * 1024 * 1024, "Archive too large")
        require(hashlib.sha256(archive.read_bytes()).hexdigest() == pin["sha256"],
                f"Archive changed: {target}")
        with zipfile.ZipFile(archive) as package:
            names = package.namelist()
            require(len(names) == len(set(names)), "Duplicate ZIP member")
            require(len(names) <= 4096, "Too many ZIP members")
            require(sum(info.file_size for info in package.infolist()) <= 512 * 1024 * 1024,
                    "Archive expansion limit")
            packaged = json.loads(package.read("extension.json"))
            require(policy(packaged) == policy(manifest), "Package policy differs from release")
            inventory = packaged["files"]
            require(bool(inventory) and set(names) == set(inventory) | {"extension.json"},
                    "ZIP must have a complete exact inventory")
            if manifest.get("files"):
                require(manifest["files"] == inventory, "Explicit trusted inventory differs")
            for name, digest in inventory.items():
                require(hashlib.sha256(package.read(name)).hexdigest() == digest,
                        f"Inventory mismatch: {name}")
            require(pin["executable"].startswith("bin/")
                    and inventory.get(pin["executable"]) == pin["executableSha256"],
                    "Executable pin differs from inventoried binary")
            if target == "aarch64-apple-darwin":
                mode = package.getinfo(pin["executable"]).external_attr >> 16
                require(stat.S_ISREG(mode) and bool(mode & 0o111),
                        "macOS executable is not a regular executable ZIP member")


def replace_objects(raw, replacements):
    """Replace selected top-level objects without reformatting unrelated records."""
    decoder = json.JSONDecoder()
    require(raw.lstrip().startswith("["), "Expected catalog array")
    index = raw.index("[") + 1
    edits = []
    found = set()
    while True:
        while index < len(raw) and raw[index] in " \t\r\n,":
            index += 1
        if raw[index] == "]":
            break
        item, end = decoder.raw_decode(raw, index)
        key = item["id"]
        require(key not in found, f"Duplicate catalog identity: {key}")
        found.add(key)
        if key in replacements:
            replacement = json.dumps(replacements[key], indent=2, ensure_ascii=False)
            newline = "\r\n" if "\r\n" in raw else "\n"
            replacement = replacement.replace("\n", newline + "  ")
            edits.append((index, end, replacement))
        index = end
    require(set(replacements) <= found, "Replacement identity missing")
    for start, end, value in reversed(edits):
        raw = raw[:start] + value + raw[end:]
    json.loads(raw)
    return raw


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--audit-dir", type=Path, default=ROOT / "docs/rust-ports-store/published-audit")
    parser.add_argument("--apply", action="store_true", help="Apply verified pins; default is read-only")
    args = parser.parse_args()
    catalog_path = ROOT / "crates/tezbar-core/src/extensions/v2_catalog.json"
    loved_path = catalog_path.with_name("loved_extensions.json")
    original = {path: path.read_bytes() for path in (catalog_path, loved_path)}
    catalog = json.loads(original[catalog_path])
    loved = json.loads(original[loved_path])
    replacements, markers, outcomes = {}, {}, []
    for report in json.loads((args.audit_dir / "audit-results.json").read_text(encoding="utf-8")):
        directory = args.audit_dir / report["slug"]
        trusted = json.loads((directory / "extension-release.json").read_text(encoding="utf-8"))
        matches = [entry for entry in catalog if entry["id"] == trusted["id"]]
        require(len(matches) == 1, "Expected exactly one existing catalog record")
        current = matches[0]
        if policy(current) != policy(trusted):
            outcomes.append({"id": trusted["id"], "status": "excluded-policy-mismatch"})
            continue
        verify(trusted, directory)
        require(not current.get("artifacts") or current["artifacts"] == trusted["artifacts"],
                "Refusing to replace different existing release pins")
        replacement = dict(current, artifacts=trusted["artifacts"])
        replacements[current["id"]] = replacement
        for row in loved:
            if row.get("nativePort", {}).get("id") == current["id"]:
                marker = dict(row["nativePort"], status="published", version=trusted["version"],
                              releaseAvailable=True)
                markers[row["id"]] = dict(row, nativePort=marker)
        outcomes.append({"id": current["id"], "status": "verified-public-dual-target",
                         "desktopAcceptance": "unchanged", "productionImporter": "separate-check-required"})
    rendered = {catalog_path: replace_objects(original[catalog_path].decode("utf-8"), replacements),
                loved_path: replace_objects(original[loved_path].decode("utf-8"), markers)}
    if args.apply:
        for path in rendered:
            require(path.read_bytes() == original[path], f"File changed during verification: {path}")
        for path, content in rendered.items():
            with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as temporary:
                temporary.write(content.encode("utf-8"))
                staged = temporary.name
            os.replace(staged, path)
    print(json.dumps({"applied": args.apply, "results": outcomes}, indent=2))


if __name__ == "__main__":
    main()
