#!/usr/bin/env python3
"""Pinned public package + native host acceptance. Standard library only; no domain actions."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
import zipfile

SDK_REV = "e955911f0bf5b558f9d3eb5183df0adc95c22af2"
SDK_REPO = "https://github.com/almatkai/tezbar-extension-sdk-rust.git"
MAX_ZIP, MAX_EXPANDED, MAX_MEMBERS = 128 * 1024**2, 512 * 1024**2, 4096
PUBLIC = {"amphetamine", "audio-device", "cleanshotx", "color-picker", "gif-search",
          "google-calendar", "google-search", "perplexity", "speedtest", "timers"}
FIXTURE_KEYS = ("TEZBAR_LAUNCHER_SDK_FIXTURE", "TEZBAR_HOST_TEST_MANIFEST", "TEZBAR_HOST_TEST_ZIP",
                "TEZBAR_HOST_TEST_CALENDAR_MANIFEST", "TEZBAR_HOST_TEST_CALENDAR_ZIP")


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def load(path):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, "Duplicate JSON key")
            result[key] = value
        return result
    return json.loads(Path(path).read_text(encoding="utf-8"), object_pairs_hook=pairs)


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def native_target():
    system, machine = platform.system(), platform.machine().lower()
    if system == "Windows" and machine in ("amd64", "x86_64"):
        return "x86_64-pc-windows-msvc"
    if system == "Darwin" and machine in ("arm64", "aarch64"):
        return "aarch64-apple-darwin"
    raise ValueError(f"Unsupported real-package target: {system}/{machine}; no Intel Mac artifact")


def safe_path(name):
    require(isinstance(name, str) and name and len(name) <= 240, "Invalid package path")
    parts = name.split("/")
    require(not any(p in ("", ".", "..") or p.endswith((" ", ".")) for p in parts), "Unsafe package path")
    require(not any(c in name for c in '\\:\x00') and not any(ord(c) < 32 for c in name), "Unsafe package path")
    reserved = {"CON", "PRN", "AUX", "NUL"} | {f"{p}{i}" for p in ("COM", "LPT") for i in range(1, 10)}
    require(not any(p.split(".")[0].upper() in reserved for p in parts), "Reserved package path")
    return name


def policy(m):
    return {k: v for k, v in m.items() if k not in ("files", "artifacts")}


def reference_matches(raw, trusted):
    # The published Calendar reference omits only the catalog's empty envelope.
    require(trusted.get("files") == {} and "files" not in raw, "Expected omitted files reference")
    expected = dict(trusted)
    del expected["files"]
    require(raw == expected, "Published reference differs from trusted catalog")


def binary_header(data, target):
    if target == "aarch64-apple-darwin":
        require(len(data) >= 32 and data[:4] == b"\xcf\xfa\xed\xfe"
                and int.from_bytes(data[4:8], "little") == 0x100000c, "Not native ARM64 Mach-O")
    elif target == "x86_64-pc-windows-msvc":
        require(len(data) >= 64 and data[:2] == b"MZ", "Not a PE executable")
        offset = int.from_bytes(data[60:64], "little")
        require(offset + 6 <= len(data) and data[offset:offset+4] == b"PE\0\0"
                and data[offset+4:offset+6] == b"\x64\x86", "Not native x64 PE")
    else:
        raise ValueError("Unsupported artifact target")


def verify_package(trusted, archive, target):
    require(trusted.get("schemaVersion") == 3 and trusted.get("runtime") == "rust"
            and trusted.get("execution") == "process" and trusted.get("protocolVersion") == 1,
            "Expected schema-3 protocol-1 process package")
    require(trusted["id"] == "com.tezbar.native." + trusted["slug"], "Identity mismatch")
    require(target in trusted["artifacts"], "No pinned target artifact")
    artifact = trusted["artifacts"][target]
    for key in ("sha256", "executableSha256"):
        require(re.fullmatch("[0-9a-f]{64}", artifact[key]) is not None, "Invalid pinned digest")
    require(Path(archive).stat().st_size <= MAX_ZIP and sha(archive) == artifact["sha256"], "Archive SHA256 mismatch")
    with zipfile.ZipFile(archive) as z:
        entries = z.infolist()
        require(len(entries) <= MAX_MEMBERS and sum(i.file_size for i in entries) <= MAX_EXPANDED, "ZIP size/count limit")
        names, files = set(), {}
        for entry in entries:
            name = safe_path(entry.filename.rstrip("/") if entry.is_dir() else entry.filename)
            require(name.casefold() not in names, "Duplicate/case-colliding ZIP path")
            names.add(name.casefold())
            mode = entry.external_attr >> 16
            require(stat.S_IFMT(mode) in (0, stat.S_IFREG, stat.S_IFDIR), "ZIP link/special file rejected")
            require(not entry.flag_bits & 1 and entry.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED), "Unsupported ZIP encoding")
            if not entry.is_dir():
                files[name] = entry
        require("extension.json" in files and files["extension.json"].file_size <= 2 * 1024**2, "Missing/oversize manifest")
        # Reuse the strict JSON decoder without extracting any package path.
        def unique(items):
            d = {}
            for k, v in items:
                require(k not in d, "Duplicate JSON key")
                d[k] = v
            return d
        packaged = json.loads(z.read("extension.json"), object_pairs_hook=unique)
        inventory = packaged.get("files")
        require(isinstance(inventory, dict) and inventory, "Missing closed inventory")
        require(set(files) == set(inventory) | {"extension.json"}, "ZIP inventory is not closed")
        require(policy(packaged) == policy(trusted), "Package identity/policy mismatch")
        trusted_files = trusted.get("files", {})
        require(isinstance(trusted_files, dict) and (not trusted_files or trusted_files == inventory), "Trusted inventory mismatch")
        require(inventory.get(artifact["executable"]) == artifact["executableSha256"], "Executable inventory mismatch")
        for name, digest in inventory.items():
            safe_path(name)
            require(re.fullmatch("[0-9a-f]{64}", digest) is not None, "Invalid inventory digest")
            h = hashlib.sha256()
            with z.open(name) as f:
                for block in iter(lambda: f.read(1024 * 1024), b""):
                    h.update(block)
            require(h.hexdigest() == digest, "Inventory digest mismatch")
        binary_header(z.read(artifact["executable"]), target)
    return inventory


class PublicRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        validate_url(newurl, redirect=True)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def validate_url(url, redirect=False):
    u = urllib.parse.urlsplit(url)
    hosts = {"github.com"}
    if redirect:
        hosts |= {"release-assets.githubusercontent.com", "objects.githubusercontent.com"}
    require(u.scheme == "https" and u.hostname in hosts and u.port in (None, 443)
            and not u.username and not u.password, "Non-public/credentialed download URL")
    return u


def download(url, destination, limit):
    validate_url(url)
    # No token, Authorization header, netrc, or private acquisition path.
    opener = urllib.request.build_opener(PublicRedirect())
    deadline = time.monotonic() + 180
    temporary = destination.with_suffix(destination.suffix + ".part")
    try:
        with opener.open(urllib.request.Request(url, headers={"User-Agent": "tezbar-native-acceptance"}), timeout=30) as response, temporary.open("wb") as f:
            validate_url(response.url, redirect=True)
            total = 0
            while True:
                require(time.monotonic() < deadline, "Download deadline exceeded")
                block = response.read1(1024 * 1024)
                if not block:
                    break
                total += len(block)
                require(total <= limit, "Download size limit")
                f.write(block)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)


def clean_fixture_env():
    env = os.environ.copy()
    for key in list(env):
        if key.startswith(("TEZBAR_HOST_TEST_", "TEZBAR_LAUNCHER_")):
            del env[key]
    env.update(CARGO_BUILD_JOBS="1", RUST_TEST_THREADS="1", RUSTUP_TOOLCHAIN="1.85.0")
    return env


def cleanup_owned_process(process, job):
    """Close the tree guard and reap Popen even when assignment never succeeded."""
    original = sys.exc_info()[1]
    errors = []

    def attempt(label, operation):
        try:
            operation()
        except BaseException as error:
            # Cleanup must continue through cancellation and preserve the first failure.
            errors.append(f"{label}: {error!r}")

    if job:
        if process is not None and process.poll() is None:
            attempt("terminate job", lambda: require(job.k.TerminateJobObject(job.handle, 1),
                                                     "TerminateJobObject failed"))
        # Kill-on-close also covers descendants if the leader already exited.
        attempt("close job", job.close)
    if process is not None:
        if process.poll() is None:
            if not job:
                attempt("kill owned group", lambda: os.killpg(process.pid, signal.SIGKILL))
            # On Windows the process may still be suspended OUTSIDE the job.
            # Popen.kill targets its owned handle, never a searched/reused PID.
            if process.poll() is None:
                attempt("kill owned process", process.kill)
        attempt("reap owned process", lambda: process.wait(timeout=5))
    if errors:
        message = "Owned process cleanup: " + "; ".join(errors)
        if original is None:
            raise RuntimeError(message)
        if hasattr(original, "add_note"):
            original.add_note(message)
        print(message, file=sys.stderr)


def bounded(argv, cwd, env, log, timeout):
    job, process = None, None
    try:
        if os.name == "nt":
            job = WindowsJob(single_process=False)
        with log.open("wb") as f:
            process = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                       stdout=f, stderr=subprocess.STDOUT,
                                       **({"creationflags": 0x4 | 0x08000000} if job else {"start_new_session": True}))
            if job:
                job.attach_resume(process)
            process.wait(timeout=timeout)
        if process.returncode:
            print(log.read_text(encoding="utf-8", errors="replace")[-16000:], file=sys.stderr)
        require(process.returncode == 0, f"Command failed ({process.returncode}); see {log.name}")
    finally:
        cleanup_owned_process(process, job)


def prepare(repo, out, local_releases=None, build_sdk=True):
    target = native_target()
    out.mkdir(parents=True, exist_ok=False)
    catalog = load(repo / "src-tauri/src/native_core/extensions/v2_catalog.json")
    acquisition = load(repo / "src-tauri/src/native_core/extensions/acquisition.json")
    require(acquisition.get("com.tezbar.native.port-manager") == "local-import-only", "Private acquisition policy changed")
    public = [m for m in catalog if m["slug"] in PUBLIC]
    require({m["slug"] for m in public} == PUBLIC and len(public) == 10, "Public catalog changed")
    require(next(m for m in public if m["slug"] == "perplexity")["version"] == "1.0.0", "Perplexity pin changed")
    require(next(m for m in public if m["slug"] == "google-calendar")["version"] == "1.4.4", "Calendar pin changed")
    records = []
    for m in public:
        a = m["artifacts"][target]
        u = validate_url(a["url"])
        require(a["url"].startswith(m["repository"] + "/releases/download/v" + m["version"] + "/"), "Artifact URL not pinned to release")
        name = Path(u.path).name
        archive = out / name
        if local_releases:
            # Validation-only cache: exact immutable pins still checked; never search Port directories.
            source = local_releases / m["slug"] / name
            if m["slug"] == "color-picker":
                source = local_releases / m["slug"] / "published-downloads" / name
            require(source.is_file(), "Missing local public archive")
            archive.write_bytes(source.read_bytes())
        else:
            download(a["url"], archive, MAX_ZIP)
        inventory = verify_package(m, archive, target)
        manifest = out / (m["slug"] + ".json")
        trusted = m
        if m["slug"] == "google-calendar":
            if local_releases:
                raw = load(local_releases / m["slug"] / "extension.json")
            else:
                download(a["url"].rsplit("/", 1)[0] + "/extension.json", manifest, 2 * 1024**2)
                raw = load(manifest)
            reference_matches(raw, m)
            trusted = raw
        save(manifest, trusted)
        records.append({"slug": m["slug"], "manifest": str(manifest), "archive": str(archive),
                        "archiveSha256": a["sha256"], "manifestSha256": sha(manifest), "members": len(inventory)})
        print(f"Verified public {m['slug']} {m['version']} {target}: {len(inventory)} members", flush=True)
    sdk = None
    if build_sdk:
        sdk_dir = out / "sdk-source"
        env = clean_fixture_env()
        bounded(["git", "init", str(sdk_dir)], out, env, out / "sdk-init.log", 30)
        bounded(["git", "-c", "credential.helper=", "fetch", "--depth=1", SDK_REPO, SDK_REV], sdk_dir,
                {**env, "GIT_TERMINAL_PROMPT": "0"}, out / "sdk-fetch.log", 180)
        bounded(["git", "checkout", "--detach", "FETCH_HEAD"], sdk_dir, env, out / "sdk-checkout.log", 30)
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=sdk_dir, timeout=10).decode().strip()
        require(head == SDK_REV, "SDK revision mismatch")
        import tomllib
        require(tomllib.loads((sdk_dir / "Cargo.toml").read_text())["package"]["version"] == "0.1.1", "SDK version mismatch")
        bounded(["cargo", "+1.85.0", "build", "--locked", "--bin", "sdk-fixture", "-j1"], sdk_dir,
                {**env, "CARGO_TARGET_DIR": str(out / "sdk-target")}, out / "sdk-build.log", 900)
        sdk = out / "sdk-target/debug" / ("sdk-fixture.exe" if os.name == "nt" else "sdk-fixture")
        binary_header(sdk.read_bytes(), target)
    save(out / "fixtures.json", {"target": target, "catalogSha256": sha(repo / "src-tauri/src/native_core/extensions/v2_catalog.json"),
                                 "sdkRevision": SDK_REV if sdk else None, "sdk": str(sdk) if sdk else None,
                                 "sdkSha256": sha(sdk) if sdk else None, "packages": records})
    return load(out / "fixtures.json")


def fixture_inputs(repo, out, need_sdk=False):
    info = load(out / "fixtures.json")
    require(info["target"] == native_target(), "Fixture target mismatch")
    require(info["catalogSha256"] == sha(repo / "src-tauri/src/native_core/extensions/v2_catalog.json"), "Catalog changed after preparation")
    for r in info["packages"]:
        require(sha(r["manifest"]) == r["manifestSha256"] and sha(r["archive"]) == r["archiveSha256"], "Prepared fixture changed")
        verify_package(load(r["manifest"]), r["archive"], info["target"])
    if need_sdk:
        require(info["sdkRevision"] == SDK_REV and info["sdk"] and sha(info["sdk"]) == info["sdkSha256"], "Missing verified SDK fixture")
    return info


def check_rust_proof(log):
    names = ("actual_sdk_package_launcher_index_fences_and_uninstall", "optional_index_activation_failure_keeps_command_sessions_available",
             "installed_package_round_trip", "published_combined_reference_omitted_files_installs_and_mismatch_rolls_back")
    for name in names:
        require(re.search(r"test [^\n]*::" + name + r" \.\.\. (?:(?!\ntest ).)*?\bok\b", log, re.S), "Missing successful Rust fixture test: " + name)
    for proof in ("Host vertical slice: real packaged SDK executable from", "Published omitted-files reference:"):
        require(proof in log, "Missing real-package proof: " + proof)
    require("SDK fixture not provided" not in log and "Published omitted-files fixture not supplied" not in log,
            "Required fixture returned early")
    require("test result: ok." in log, "Rust suite did not pass")


def rust_tests(repo, out, published=True):
    if published:
        info = fixture_inputs(repo, out, need_sdk=True)
        by_slug = {r["slug"]: r for r in info["packages"]}
    else:
        require(platform.system() == "Darwin" and platform.machine().lower() in ("x86_64", "amd64"), "Intel-only unsupported branch")
        out.mkdir(parents=True, exist_ok=False)
    env = clean_fixture_env()
    # The existing supervisor fixture explicitly passes -j2, overriding Cargo's env.
    # Compile only a tiny CI shim; the app/library/fixture sources are untouched.
    real_cargo = shutil.which("cargo")
    require(real_cargo is not None, "Cargo unavailable")
    shim = out / "cargo-shim"
    shim.mkdir()
    bounded(["rustc", "+1.85.0", "--edition=2021", str(repo / "scripts/ci-cargo-one-job.rs"),
             "-o", str(shim / ("cargo.exe" if os.name == "nt" else "cargo"))], repo, env, out / "cargo-shim.log", 60)
    env.update(TEZBAR_CI_REAL_CARGO=real_cargo, PATH=str(shim) + os.pathsep + env["PATH"])
    if published:
        env.update(dict(zip(FIXTURE_KEYS, (info["sdk"], by_slug["perplexity"]["manifest"], by_slug["perplexity"]["archive"],
                                          by_slug["google-calendar"]["manifest"], by_slug["google-calendar"]["archive"]))))
    log = out / "rust-library.log"
    try:
        bounded([real_cargo, "+1.85.0", "test", "--lib", "--locked", "-j1", "--", "--test-threads=1", "--nocapture"],
                repo / "src-tauri", env, log, 1800)
    finally:
        text = log.read_text(encoding="utf-8", errors="replace")
        print(text, end="")
    if published:
        check_rust_proof(text)
        print("REQUIRED FIXTURE BRANCHES PASSED: native SDK (2), public Perplexity, raw Calendar")
    else:
        print("UNSUPPORTED real-package acceptance: Intel Mac has no published artifact. Normal full Rust library passed; optional fixtures not claimed.")
    print("PENDING: actual private Port package/launcher fixtures are unavailable to public CI; synthetic private/local-only/rollback tests remain enabled.")


class WindowsJob:
    """Own a suspended process tree; CLI mode additionally prohibits child creation."""
    def __init__(self, single_process=True):
        from ctypes import wintypes as w
        class BasicLimits(ctypes.Structure):
            _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64), ("PerJobUserTimeLimit", ctypes.c_int64),
                        ("LimitFlags", ctypes.c_uint32), ("MinimumWorkingSetSize", ctypes.c_size_t),
                        ("MaximumWorkingSetSize", ctypes.c_size_t), ("ActiveProcessLimit", ctypes.c_uint32),
                        ("Affinity", ctypes.c_size_t), ("PriorityClass", ctypes.c_uint32), ("SchedulingClass", ctypes.c_uint32)]
        class IO(ctypes.Structure):
            _fields_ = [(name, ctypes.c_uint64) for name in ("ReadOperationCount", "WriteOperationCount", "OtherOperationCount", "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]
        class Extended(ctypes.Structure):
            _fields_ = [("BasicLimitInformation", BasicLimits), ("IoInfo", IO), ("ProcessMemoryLimit", ctypes.c_size_t),
                        ("JobMemoryLimit", ctypes.c_size_t), ("PeakProcessMemoryUsed", ctypes.c_size_t), ("PeakJobMemoryUsed", ctypes.c_size_t)]
        class Accounting(ctypes.Structure):
            # Win32 DWORD is always 32 bits, including in non-Windows ABI fakes.
            _fields_ = [(n, ctypes.c_int64) for n in ("TotalUserTime", "TotalKernelTime", "ThisPeriodTotalUserTime", "ThisPeriodTotalKernelTime")] + [(n, ctypes.c_uint32) for n in ("TotalPageFaultCount", "TotalProcesses", "ActiveProcesses", "TotalTerminatedProcesses")]
        self.accounting = Accounting
        self.k = ctypes.WinDLL("kernel32", use_last_error=True)
        self.k.CreateJobObjectW.argtypes = [ctypes.c_void_p, w.LPCWSTR]
        self.k.CreateJobObjectW.restype = w.HANDLE
        self.k.SetInformationJobObject.argtypes = [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD]
        self.k.AssignProcessToJobObject.argtypes = [w.HANDLE, w.HANDLE]
        self.k.QueryInformationJobObject.argtypes = [w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p]
        self.k.CloseHandle.argtypes = [w.HANDLE]
        self.k.TerminateJobObject.argtypes = [w.HANDLE, w.UINT]
        self.handle = self.k.CreateJobObjectW(None, None)
        require(self.handle, "CreateJobObject failed")
        try:
            limits = Extended()
            limits.BasicLimitInformation.LimitFlags = 0x2000  # kill-on-close
            if single_process:
                limits.BasicLimitInformation.LimitFlags |= 0x8
                limits.BasicLimitInformation.ActiveProcessLimit = 1
            require(self.k.SetInformationJobObject(self.handle, 9, ctypes.byref(limits), ctypes.sizeof(limits)), "SetInformationJobObject failed")
        except BaseException:
            cleanup_owned_process(None, self)
            raise

    def attach_resume(self, process):
        require(self.k.AssignProcessToJobObject(self.handle, int(process._handle)), "AssignProcessToJobObject failed")
        # Popen closes the primary thread handle. Resume only this owned suspended process.
        ntdll = ctypes.WinDLL("ntdll")
        ntdll.NtResumeProcess.argtypes = [ctypes.c_void_p]
        ntdll.NtResumeProcess.restype = ctypes.c_long
        require(ntdll.NtResumeProcess(int(process._handle)) == 0, "NtResumeProcess failed")

    def counts(self):
        data = self.accounting()
        size = ctypes.sizeof(data)
        require(size == 48, f"Job accounting ABI size mismatch: {size}")
        returned = ctypes.c_uint32()
        require(self.k.QueryInformationJobObject(self.handle, 1, ctypes.byref(data), size, ctypes.byref(returned)),
                "Job accounting query failed")
        require(returned.value == size, f"Job accounting size mismatch: returned={returned.value}, expected={size}")
        return {name: getattr(data, name) for name in ("TotalProcesses", "ActiveProcesses", "TotalTerminatedProcesses")}

    def close(self):
        if self.handle:
            require(self.k.CloseHandle(self.handle), "CloseHandle failed")
            self.handle = None


def cli_environment(profile):
    env = clean_fixture_env()
    env.update(TEZBAR_DATA_DIR=str(profile / "data"), TEZBAR_CONFIG_DIR=str(profile / "config"),
               TMPDIR=str(profile / "tmp"), TEMP=str(profile / "tmp"), TMP=str(profile / "tmp"))
    for key in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
        env[key] = "http://127.0.0.1:9"
    env.update(NO_PROXY="", no_proxy="")
    return env


def guarded_run(argv, profile, env, out, label, expected=0, *, _guard_selfcheck_marker=None):
    job, process = None, None
    start = time.monotonic()
    stdout, stderr = out / (label + ".stdout.json"), out / (label + ".stderr.log")
    try:
        if os.name == "nt":
            job = WindowsJob()
        else:
            sandbox = out / "cli.sb"
            sandbox.write_text("(version 1)\n(allow default)\n(deny network*)\n(deny process-fork)\n", encoding="utf-8")
            argv = ["/usr/bin/sandbox-exec", "-f", str(sandbox), *argv]
        with stdout.open("wb") as o, stderr.open("wb") as e:
            process = subprocess.Popen(argv, cwd=profile, env=env, stdin=subprocess.DEVNULL, stdout=o, stderr=e,
                                       **({"creationflags": 0x4 | 0x08000000} if job else {"start_new_session": True}))
            if job:
                job.attach_resume(process)
            process.wait(timeout=30)
        counts = job.counts() if job else None
        diagnostic = f"; Job accounting: {json.dumps(counts, sort_keys=True)}" if counts is not None else ""
        if job:
            # Print before checking output/exit status so failed native probes retain evidence.
            print(f"Windows Job {label}{diagnostic}", flush=True)
            total, active, terminated = (counts[n] for n in ("TotalProcesses", "ActiveProcesses", "TotalTerminatedProcesses"))
            require(total >= 1 and active <= total and terminated <= total, "Invalid Job accounting" + diagnostic)
            if _guard_selfcheck_marker is None:
                require(total == 1, "CLI spawned additional Job processes" + diagnostic)
                require(terminated == 0, "Invalid Job accounting: CLI limit termination" + diagnostic)
        require(process.returncode == expected, f"CLI {label}: expected {expected}, got {process.returncode}; see local logs" + diagnostic)
        try:
            result = load(stdout)
        except ValueError as error:
            raise ValueError(f"CLI {label}: invalid JSON output" + diagnostic) from error
        if _guard_selfcheck_marker is not None:
            require(isinstance(result, dict) and result.get("childDenied") is True, "Child guard ineffective" + diagnostic)
            require(type(result.get("childAttempts")) is int and result["childAttempts"] == 1
                    and result.get("childMarker") == str(_guard_selfcheck_marker), "Invalid child attempt proof" + diagnostic)
            error = result.get("childError")
            require(isinstance(error, dict) and set(error) == {"errno", "winerror"}
                    and all(value is None or type(value) is int for value in error.values()),
                    "Invalid child OSError proof" + diagnostic)
            print(f"Guard child OSError: {json.dumps(error, sort_keys=True)}", flush=True)
            if job:
                # Native Windows observed total=2, active=1, terminated=0 after OSError.
                # Termination accounting alone cannot prove CreateProcess rejection.
                # Require one association attempt plus denial AND post-cleanup nonexecution.
                require(total == 2 and terminated in (0, 1), "Self-check Job accounting: expected one denied child" + diagnostic)
                result["jobAccounting"] = counts
    finally:
        cleanup_owned_process(process, job)
    if _guard_selfcheck_marker is not None:
        # Check only AFTER Job closure/reap, while the parent-owned directory still
        # exists: even partial/late child execution must leave persistent evidence.
        require(_guard_selfcheck_marker.parent.is_dir(), "Child marker directory missing after cleanup" + diagnostic)
        require(not _guard_selfcheck_marker.exists() and not _guard_selfcheck_marker.is_symlink(),
                "Child execution marker found after cleanup" + diagnostic)
        result["childExecutionAbsent"] = True
    print(f"CLI {label}: exit={process.returncode}, child creation prohibited, {time.monotonic()-start:.3f}s", flush=True)
    return result


def guard_selfcheck(profile, env, out):
    # Owned inert Python process must fail both attempted child creation and (Mac only) networking.
    code = '''import json,subprocess,sys,socket
marker=sys.argv[1]
attempts=0
error=None
try:
 attempts+=1
 subprocess.run([sys.executable,"-c","import sys; f=open(sys.argv[1], 'xb'); f.write(b'child executed'); f.close()",marker],check=True,timeout=5)
 child=False
except OSError as exc:
 child=True
 error={"errno":exc.errno,"winerror":getattr(exc,"winerror",None)}
network=None
if sys.platform=="darwin":
 try:
  s=socket.socket()
  s.settimeout(.2)
  s.connect(("127.0.0.1",9))
  network=False
 except PermissionError:
  network=True
print(json.dumps({"childDenied":child,"childAttempts":attempts,"childMarker":marker,"childError":error,"networkDenied":network}))
'''
    with tempfile.TemporaryDirectory(prefix="guard-child-", dir=out.resolve()) as directory:
        marker = Path(directory) / "executed"
        proof = guarded_run([sys.executable, "-c", code, str(marker)], profile, env, out, "guard-selfcheck",
                            _guard_selfcheck_marker=marker)
    require(isinstance(proof, dict) and proof.get("childDenied") is True, "Child guard ineffective")
    if os.name != "nt":
        require(proof["networkDenied"] is True, "Mac network sandbox ineffective")
    return proof


def tree(root):
    require(not any(p.is_symlink() for p in root.rglob("*")), "Profile symlink")
    return {p.relative_to(root).as_posix(): sha(p) for p in root.rglob("*") if p.is_file()}


def installed_inventory(profile, trusted, archive, target):
    inventory = verify_package(trusted, archive, target)
    root = profile / "data/extensions/packages" / trusted["id"]
    pointer = load(root / "active.json")
    require(re.fullmatch(r"[A-Za-z0-9._-]+", pointer["directory"]) and pointer["directory"] not in (".", ".."), "Invalid pointer directory")
    directory = root / "versions" / pointer["directory"]
    require(sha(directory / "extension.json") == pointer["manifestSha256"], "Active digest mismatch")
    expected = dict(trusted, files=inventory)
    require(load(directory / "extension.json") == expected, "Installed policy/inventory mismatch")
    require(tree(directory) == {**inventory, "extension.json": pointer["manifestSha256"]}, "Installed closed inventory mismatch")
    artifact = trusted["artifacts"][target]
    require(sha(directory / artifact["executable"]) == artifact["executableSha256"], "Installed executable digest mismatch")
    if os.name != "nt":
        require((directory / artifact["executable"]).stat().st_mode & 0o111, "Executable permission missing")
    return {"id": trusted["id"], "pointer": pointer, "inventoryMembers": len(inventory), "executableSha256": artifact["executableSha256"]}


def cli(repo, out, binary):
    info = fixture_inputs(repo, out)
    require(binary.is_file(), "Production CLI missing")
    binary_header(binary.read_bytes(), info["target"])
    profile = out / "cli-profile"
    profile.mkdir(exist_ok=False)
    for name in ("data", "config", "tmp"):
        (profile / name).mkdir()
    env = cli_environment(profile)
    guard = guard_selfcheck(profile, env, out)
    results = []
    def run(label, args, expected=0):
        value = guarded_run([str(binary), *args], profile, env, out, label, expected)
        results.append({"label": label, "expectedExit": expected})
        return value
    initial = run("list-empty", ["--extensions-v2-list", "--json"])
    catalog = load(repo / "src-tauri/src/native_core/extensions/v2_catalog.json")
    require(len(initial) == len(catalog) and all(not m["installed"] for m in initial), "CLI profile not empty")
    port = next(m for m in initial if m["slug"] == "port-manager")
    require(port["acquisition"]["type"] == "local-import-only" and port["status"] == "unavailable", "Private acquisition metadata mismatch")
    before = tree(profile)
    refused = run("private-remote-refused", ["--extensions-v2-install", port["id"]], 1)
    require("--extensions-v3-import" in json.dumps(refused) and tree(profile) == before, "Private refusal changed profile")
    installed = []
    for r in info["packages"]:
        m = load(r["manifest"])
        imported = run("import-" + r["slug"], ["--extensions-v3-import", r["manifest"], r["archive"]])
        require(imported.get("ok") is True, "Local import rejected")
        installed.append(installed_inventory(profile, m, r["archive"], info["target"]))
    listing = run("list-installed", ["--extensions-v2-list", "--json"])
    gaps = []
    host_platform = "Windows" if os.name == "nt" else "macOS"
    for r in info["packages"]:
        m = load(r["manifest"])
        item = next(i for i in listing if i["id"] == m["id"])
        covered = [c["name"] for c in m["commands"] if c.get("runtime") == "rust" and host_platform in c.get("platforms", [host_platform])]
        expected = [(c["name"], "rust" if c["name"] in covered else "unavailable") for c in m["commands"]]
        status = "complete" if len(covered) == len(expected) else "partial" if covered else "unavailable"
        require(item["installed"] and item["installationStatus"] == "installed" and item["status"] == status, "Discovery/platform coverage mismatch")
        require([(c["name"], c["runtime"]) for c in item["commands"]] == expected, "Command metadata coverage mismatch")
        gaps.append({"slug": r["slug"], "status": status, "unavailable": [n for n, runtime in expected if runtime != "rust"]})
    r = next(r for r in info["packages"] if r["slug"] == "perplexity")
    m = load(r["manifest"])
    prefs = profile / "data/extensions/packages" / m["id"] / "preferences.json"
    save(prefs, {"extension": {"ownedSentinel": "preserve-Қазақша"}, "commands": {"ask-perplexity": {"owned": True}}})
    before = tree(profile)
    bad = out / "bad-sha-public.zip"
    bad.write_bytes(Path(r["archive"]).read_bytes() + b"owned-bad-sha")
    rejected = run("bad-sha-rollback", ["--extensions-v3-import", r["manifest"], str(bad)], 1)
    require("Archive SHA256 mismatch" in json.dumps(rejected) and tree(profile) == before, "Bad SHA changed pointer/preferences/profile")
    run("list-after-rollback", ["--extensions-v2-list", "--json"])
    require(tree(profile) == before, "Listing changed profile")
    summary = {"target": info["target"], "binarySha256": sha(binary), "binaryBytes": binary.stat().st_size,
               "publicPackages": installed, "platformGaps": gaps, "cliInvocations": results,
               "guardSelfcheck": guard, "childCreationProhibited": True,
               "networkPolicy": "Mac sandbox deny network*" if os.name != "nt" else "process-scoped unreachable proxies; no hard network block",
               "privateRealPackageAcceptance": "PENDING: Windows private package; no private bytes acquired; Mac prior acceptance is separate evidence",
               "scope": "CLI metadata/import/integrity only; library supervisor tests separate; no NSIS/live desktop QA"}
    save(out / "cli-summary.json", summary)
    print(json.dumps(summary, indent=2, ensure_ascii=False))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("action", choices=("prepare", "rust-tests", "rust-tests-intel", "cli", "validate-local"))
    p.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--binary", type=Path)
    p.add_argument("--local-releases", type=Path)
    args = p.parse_args()
    repo, out = args.repo.resolve(), args.out.resolve()
    require(not out.is_relative_to(repo), "Acceptance output must be outside repository")
    if args.action == "prepare":
        require(args.local_releases is None, "CI preparation requires public immutable downloads")
        prepare(repo, out)
    elif args.action == "validate-local":
        require(args.local_releases is not None, "Validation requires existing verified public files")
        prepare(repo, out, args.local_releases.resolve(), build_sdk=False)
        if args.binary:
            cli(repo, out, args.binary.resolve())
        print("Local helper validation only: no SDK build or Rust-suite/post-review acceptance claimed")
    elif args.action in ("rust-tests", "rust-tests-intel"):
        rust_tests(repo, out, published=args.action == "rust-tests")
    else:
        require(args.binary is not None, "--binary required")
        cli(repo, out, args.binary.resolve())


if __name__ == "__main__":
    main()
