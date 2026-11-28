"""Exercise published release manifests with the current production importer.

All imports use disposable OS-temp profiles. No installed user extension, saved
preference, clipboard or window is touched. Build the headless helper locally first.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-pc-windows-msvc"


def checked_run(command, env):
    result = subprocess.run(command, env=env, capture_output=True, text=True,
                            encoding="utf-8", errors="replace", timeout=120)
    if result.returncode:
        raise ValueError(f"Command failed ({result.returncode}): {result.stdout} {result.stderr}")
    return result.stdout


def verify(host, directory, current):
    trusted_path = directory / "extension-release.json"
    trusted = json.loads(trusted_path.read_text(encoding="utf-8"))
    if current != trusted:
        # The combined public manifest may omit/empty files; every other field
        # must be exactly the policy already integrated by the parent.
        policy = lambda value: {key: item for key, item in value.items() if key != "files"}
        if policy(current) != policy(trusted):
            raise ValueError("Current catalog differs from public trusted manifest")
    pin = trusted["artifacts"][TARGET]
    archive = directory / pin["url"].rsplit("/", 1)[-1]
    if hashlib.sha256(archive.read_bytes()).hexdigest() != pin["sha256"]:
        raise ValueError("Published archive changed")
    with tempfile.TemporaryDirectory(prefix="pub-port-", ignore_cleanup_errors=True) as temp:
        root = Path(temp)
        env = {key: os.environ[key] for key in ("SystemRoot", "WINDIR", "PATH") if key in os.environ}
        env.update(USERPROFILE=str(root), HOME=str(root), APPDATA=str(root / "roaming"),
                   LOCALAPPDATA=str(root / "local"), TEMP=str(root), TMP=str(root),
                   TEZBAR_DATA_DIR=str(root / "data"), TEZBAR_CONFIG_DIR=str(root / "config"))
        checked_run([str(host), "--extensions-v3-import", str(trusted_path), str(archive)], env)
        active = root / "data/extensions/packages" / trusted["id"] / "active.json"
        pointer = json.loads(active.read_text(encoding="utf-8"))
        installed = active.parent / "versions" / pointer["directory"]
        binary = installed / pin["executable"]
        if hashlib.sha256(binary.read_bytes()).hexdigest() != pin["executableSha256"]:
            raise ValueError("Installed executable digest mismatch")
        info = json.loads(checked_run([str(binary), "--protocol-info"], env))
        expected = {command["name"] for command in trusted["commands"]
                    if command["runtime"] == "rust" and "Windows" in command["platforms"]}
        if (info["id"], info["version"], info["protocol"], set(info["commands"])) != (
                trusted["id"], trusted["version"], 1, expected):
            raise ValueError("Installed executable identity/command coverage mismatch")
        # This uses the newly compiled catalog, not merely the supplied manifest.
        listing = json.loads(checked_run([str(host), "--extensions-v2-list", "--json"], env))
        rows = listing if isinstance(listing, list) else listing.get("extensions", listing.get("results", []))
        matches = [row for row in rows if row.get("id") == trusted["id"]]
        if len(matches) != 1 or matches[0].get("installationStatus") != "installed":
            raise ValueError("Compiled catalog did not resolve installed published package")
        return {"id": trusted["id"], "version": trusted["version"], "target": TARGET,
                "archiveSha256": pin["sha256"], "productionImport": "passed",
                "installedProtocolInfo": "passed", "compiledCatalogDiscovery": "passed",
                "commands": sorted(expected), "desktopAcceptance": "not-run"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--audit-dir", type=Path, default=ROOT / "docs/rust-ports-store/published-audit")
    parser.add_argument("--published-dir", type=Path, action="append", default=[],
                        help="Additional directory containing a pinned manifest and Windows archive")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("This check executes Windows packages and requires Windows")
    host = args.host.resolve(strict=True)
    catalog = json.loads((ROOT / "crates/tezbar-core/src/extensions/v2_catalog.json").read_text(encoding="utf-8"))
    results = []
    directories = [args.audit_dir / report["slug"] for report in
                   json.loads((args.audit_dir / "audit-results.json").read_text(encoding="utf-8"))
                   if report.get("catalogPolicy") == "passed"]
    for directory in directories + args.published_dir:
        slug = json.loads((directory / "extension-release.json").read_text(encoding="utf-8"))["slug"]
        entry = next(item for item in catalog if item["slug"] == slug)
        try:
            results.append(verify(host, directory, entry))
        except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
            results.append({"id": entry["id"], "error": str(error)})
    args.report.write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(results, indent=2))
    return int(not results or any("error" in result for result in results))


if __name__ == "__main__":
    raise SystemExit(main())
