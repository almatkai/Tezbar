"""Acceptance boundaries using inert, tiny archives; no network, Cargo, or domain execution."""
import copy
import contextlib
import ctypes
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import struct
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import zipfile

spec = importlib.util.spec_from_file_location("native_host_acceptance", Path(__file__).with_name("native-host-acceptance.py"))
a = importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)
ARM, WIN = "aarch64-apple-darwin", "x86_64-pc-windows-msvc"


class PackageBoundary(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "tiny.zip"
        self.binary = b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x100000c) + b"\0" * 24
        self.trusted = {"schemaVersion": 3, "runtime": "rust", "execution": "process", "protocolVersion": 1,
                        "id": "com.tezbar.native.test", "slug": "test", "version": "1.0.0",
                        "commands": [{"name": "inert"}], "capabilities": [], "files": {}, "artifacts": {}}

    def archive(self, mutate=None, extra=None, executable=None):
        binary = self.binary if executable is None else executable
        packaged = copy.deepcopy(self.trusted)
        packaged["files"] = {"bin/test": hashlib.sha256(binary).hexdigest()}
        if mutate:
            mutate(packaged)
        with zipfile.ZipFile(self.path, "w") as z:
            z.writestr("extension.json", json.dumps(packaged))
            z.writestr("bin/test", binary)
            if extra:
                z.writestr(extra, b"unexpected")
        trusted = copy.deepcopy(self.trusted)
        trusted["artifacts"][ARM] = {"url": "https://github.com/owner/test/releases/download/v1.0.0/test.zip",
                                     "sha256": a.sha(self.path), "executable": "bin/test",
                                     "executableSha256": hashlib.sha256(binary).hexdigest()}
        return trusted

    def test_valid_closed_fixture_then_corruption_is_rejected(self):
        trusted = self.archive()
        self.assertEqual(set(a.verify_package(trusted, self.path, ARM)), {"bin/test"})
        with self.path.open("ab") as f:
            f.write(b"corrupted")
        with self.assertRaisesRegex(ValueError, "Archive SHA256"):
            a.verify_package(trusted, self.path, ARM)

    def test_authentic_archive_cannot_widen_grants_or_change_identity(self):
        for field, value in (("capabilities", ["shell.open"]), ("id", "com.tezbar.native.other"),
                             ("commands", [{"name": "inert", "timeoutMs": 300000}])):
            with self.subTest(field=field):
                trusted = self.archive(lambda m: m.update({field: value}))
                with self.assertRaisesRegex(ValueError, "identity/policy"):
                    a.verify_package(trusted, self.path, ARM)

    def test_header_and_target_are_independent_of_trusted_hash(self):
        trusted = self.archive(executable=b"MZ" + b"\0" * 64)
        with self.assertRaisesRegex(ValueError, "ARM64"):
            a.verify_package(trusted, self.path, ARM)
        trusted = self.archive()
        with self.assertRaisesRegex(ValueError, "No pinned target"):
            a.verify_package(trusted, self.path, WIN)
        wrong_arch = b"\xcf\xfa\xed\xfe" + struct.pack("<I", 0x1000007) + b"\0" * 24
        trusted = self.archive(executable=wrong_arch)
        with self.assertRaisesRegex(ValueError, "ARM64"):
            a.verify_package(trusted, self.path, ARM)
        pe = bytearray(90)
        pe[:2], pe[60:64], pe[64:68], pe[68:70] = b"MZ", struct.pack("<I", 64), b"PE\0\0", b"\x64\x86"
        a.binary_header(pe, WIN)
        pe[68:70] = b"\x64\xaa"
        with self.assertRaisesRegex(ValueError, "x64 PE"):
            a.binary_header(pe, WIN)

    def test_closed_inventory_rejects_unlisted_and_unsafe_paths(self):
        for name in ("resources/extra", "../escape", "C:/escape", "bin/TEST"):
            with self.subTest(name=name):
                trusted = self.archive(extra=name)
                with self.assertRaises(ValueError):
                    a.verify_package(trusted, self.path, ARM)

    def test_inventory_digest_and_trusted_nonempty_inventory_are_authoritative(self):
        trusted = self.archive(lambda m: m["files"].update({"bin/test": "0" * 64}))
        with self.assertRaisesRegex(ValueError, "Executable inventory"):
            a.verify_package(trusted, self.path, ARM)
        trusted = self.archive()
        trusted["files"] = {"bin/test": "0" * 64}
        with self.assertRaisesRegex(ValueError, "Trusted inventory"):
            a.verify_package(trusted, self.path, ARM)
        trusted["files"] = None
        with self.assertRaisesRegex(ValueError, "Trusted inventory"):
            a.verify_package(trusted, self.path, ARM)

    def test_link_and_oversized_archive_rejected_before_use(self):
        trusted = self.archive()
        info = zipfile.ZipInfo("link")
        info.create_system = 3
        info.external_attr = 0o120777 << 16
        with zipfile.ZipFile(self.path, "a") as z:
            z.writestr(info, "bin/test")
        trusted["artifacts"][ARM]["sha256"] = a.sha(self.path)
        with self.assertRaisesRegex(ValueError, "link/special"):
            a.verify_package(trusted, self.path, ARM)
        original = a.MAX_ZIP
        try:
            a.MAX_ZIP = 1
            with self.assertRaisesRegex(ValueError, "Archive SHA256"):
                a.verify_package(trusted, self.path, ARM)
        finally:
            a.MAX_ZIP = original

    def test_raw_reference_omission_does_not_authorize_remote_policy(self):
        trusted = self.archive()
        raw = copy.deepcopy(trusted)
        del raw["files"]
        a.reference_matches(raw, trusted)
        raw["capabilities"] = ["shell.open"]
        with self.assertRaisesRegex(ValueError, "differs from trusted"):
            a.reference_matches(raw, trusted)

    def test_download_origin_policy_rejects_private_or_credentialed_transport(self):
        for url in ("http://github.com/o/r", "https://token@github.com/o/r", "https://evil.example/r",
                    "https://github.com:444/o/r", "https://github.com.evil.example/r"):
            with self.subTest(url=url), self.assertRaises(ValueError):
                a.validate_url(url)
        a.validate_url("https://github.com/o/r/releases/download/v1/pkg.zip")


class RequiredBranchProof(unittest.TestCase):
    def test_early_return_or_missing_real_branch_cannot_pass(self):
        names = ("actual_sdk_package_launcher_index_fences_and_uninstall", "optional_index_activation_failure_keeps_command_sessions_available",
                 "installed_package_round_trip", "published_combined_reference_omitted_files_installs_and_mismatch_rolls_back")
        good = "\n".join("test native::" + n + " ... ok" for n in names)
        good += "\nHost vertical slice: real packaged SDK executable from pinned\nPublished omitted-files reference: pinned\ntest result: ok."
        a.check_rust_proof(good)
        for bad in (good.replace("real packaged SDK executable", "synthetic executable"),
                    good + "\nSDK fixture not provided", good.replace(names[1], "not_the_required_test")):
            with self.subTest(log=bad), self.assertRaises(ValueError):
                a.check_rust_proof(bad)


class WindowsCleanup(unittest.TestCase):
    """Run both real runners and attach_resume with inert, deterministic WinAPI fakes."""
    def exercise(self, runner, stage, termination="false", close_fails=False, kill_fails=False):
        events = []
        process = mock.Mock(pid=123, _handle=456, returncode=None)
        alive = [True]
        assigned = [False]
        process.poll.side_effect = lambda: None if alive[0] else 1

        def kill():
            events.append("kill-owned")
            if kill_fails:
                raise OSError("owned kill failed")
            alive[0], process.returncode = False, 1

        def wait(timeout):
            events.append(("wait", timeout))
            if alive[0]:
                raise a.subprocess.TimeoutExpired("owned-inert-process", timeout)
            return process.returncode

        process.kill.side_effect, process.wait.side_effect = kill, wait
        job = a.WindowsJob.__new__(a.WindowsJob)
        job.handle, job.k = 7, mock.Mock()

        def assign(*args):
            assigned[0] = stage != "assign"
            return int(assigned[0])

        def terminate(*args):
            events.append("terminate-job")
            if termination == "raise":
                raise OSError("job termination failed")
            if termination == "success" and assigned[0]:
                alive[0], process.returncode = False, 1
            return int(termination == "success")

        def close(*args):
            events.append("close-job")
            if close_fails:
                raise OSError("job close failed")
            if assigned[0]:
                alive[0], process.returncode = False, 1
            return 1

        job.k.AssignProcessToJobObject.side_effect = assign
        job.k.TerminateJobObject.side_effect = terminate
        job.k.CloseHandle.side_effect = close
        ntdll = mock.Mock()
        ntdll.NtResumeProcess.return_value = -1 if stage == "resume" else 0
        dll = mock.Mock(return_value=ntdll)
        if stage == "resume-setup":
            dll.side_effect = OSError("resume setup failed")
        expected = {"assign": (ValueError, "AssignProcessToJobObject failed"),
                    "resume": (ValueError, "NtResumeProcess failed"),
                    "resume-setup": (OSError, "resume setup failed"),
                    "wait": (a.subprocess.TimeoutExpired, "owned-inert-process")}[stage]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            diagnostics = io.StringIO()
            with mock.patch.object(a, "os", SimpleNamespace(name="nt")), \
                    mock.patch.object(a, "WindowsJob", return_value=job), \
                    mock.patch.object(a.subprocess, "Popen", return_value=process) as popen, \
                    mock.patch.object(a.ctypes, "WinDLL", dll, create=True), \
                    mock.patch.object(a.sys, "stderr", diagnostics):
                with self.assertRaisesRegex(*expected) as raised:
                    if runner == "bounded":
                        a.bounded(["owned-inert-process"], root, {}, root / "command.log", 17)
                    else:
                        a.guarded_run(["owned-inert-process"], root, {}, root, "command")
                self.assertEqual(popen.call_args.kwargs["creationflags"], 0x4 | 0x08000000)
                self.assertIn("close-job", events)
                self.assertIn(("wait", 5), events)
                if kill_fails:
                    self.assertIn("Owned process cleanup:", diagnostics.getvalue())
                else:
                    self.assertFalse(alive[0], events)
                if stage == "assign" or close_fails:
                    process.kill.assert_called_once_with()
                if stage == "wait":
                    self.assertEqual(events[0], ("wait", 17 if runner == "bounded" else 30))
        return events

    def test_assignment_failure_reaps_owned_unassigned_process_in_both_runners(self):
        for runner in ("bounded", "guarded_run"):
            for termination in ("success", "false", "raise"):
                with self.subTest(runner=runner, termination=termination):
                    self.exercise(runner, "assign", termination)

    def test_resume_failure_closes_job_and_reaps_in_both_runners(self):
        for runner in ("bounded", "guarded_run"):
            for termination in ("false", "raise"):
                with self.subTest(runner=runner, termination=termination):
                    self.exercise(runner, "resume", termination)

    def test_resume_setup_failure_preserves_original_error_in_both_runners(self):
        for runner in ("bounded", "guarded_run"):
            with self.subTest(runner=runner):
                self.exercise(runner, "resume-setup")

    def test_termination_and_close_failure_still_kills_owned_process(self):
        for runner in ("bounded", "guarded_run"):
            with self.subTest(runner=runner):
                self.exercise(runner, "wait", "raise", close_fails=True)

    def test_unreapable_process_is_bounded_and_cannot_mask_original_error(self):
        for runner in ("bounded", "guarded_run"):
            with self.subTest(runner=runner):
                self.exercise(runner, "assign", "raise", close_fails=True, kill_fails=True)

    def test_constructor_failure_closes_partially_created_job_even_on_cancellation(self):
        kernel = mock.Mock()
        kernel.CreateJobObjectW.return_value = 7
        kernel.SetInformationJobObject.side_effect = KeyboardInterrupt("cancel setup")
        with mock.patch.object(a.ctypes, "WinDLL", return_value=kernel, create=True):
            with self.assertRaisesRegex(KeyboardInterrupt, "cancel setup"):
                a.WindowsJob()
        kernel.CloseHandle.assert_called_once_with(7)

    def test_popen_failure_closes_empty_job_in_both_runners(self):
        for runner in ("bounded", "guarded_run"):
            with self.subTest(runner=runner), tempfile.TemporaryDirectory() as directory:
                root, job = Path(directory), mock.Mock()
                with mock.patch.object(a, "os", SimpleNamespace(name="nt")), \
                        mock.patch.object(a, "WindowsJob", return_value=job), \
                        mock.patch.object(a.subprocess, "Popen", side_effect=OSError("create failed")):
                    with self.assertRaisesRegex(OSError, "create failed"):
                        if runner == "bounded":
                            a.bounded(["inert"], root, {}, root / "command.log", 17)
                        else:
                            a.guarded_run(["inert"], root, {}, root, "command")
                job.close.assert_called_once_with()
                job.attach_resume.assert_not_called()

    def test_cli_accounting_rejects_extra_process_and_query_failure(self):
        for accounting in (2, OSError("Job accounting failed")):
            with self.subTest(accounting=accounting), tempfile.TemporaryDirectory() as directory:
                root, job = Path(directory), mock.Mock()
                process = mock.Mock(returncode=0)
                process.poll.return_value = 0
                if isinstance(accounting, Exception):
                    job.counts.side_effect = accounting
                    expected = (OSError, "Job accounting failed")
                else:
                    job.counts.return_value = {"TotalProcesses": accounting, "ActiveProcesses": 0, "TotalTerminatedProcesses": 0}
                    expected = (ValueError, "CLI spawned additional Job processes")
                with mock.patch.object(a, "os", SimpleNamespace(name="nt")), \
                        mock.patch.object(a, "WindowsJob", return_value=job), \
                        mock.patch.object(a.subprocess, "Popen", return_value=process):
                    with self.assertRaisesRegex(*expected):
                        a.guarded_run(["inert"], root, {}, root, "command")
                job.close.assert_called_once_with()
                self.assertEqual(process.wait.call_args_list, [mock.call(timeout=30), mock.call(timeout=5)])
                process.kill.assert_not_called()

    def test_job_limits_keep_kill_on_close_and_cli_active_process_limit(self):
        for single in (True, False):
            with self.subTest(single_process=single):
                kernel = mock.Mock()
                kernel.CreateJobObjectW.return_value = 7
                limits = []

                def set_information(handle, kind, pointer, size):
                    basic = pointer._obj.BasicLimitInformation
                    self.assertEqual(size, 144)  # Win64 JOBOBJECT_EXTENDED_LIMIT_INFORMATION
                    self.assertEqual(type(basic).LimitFlags.offset, 16)
                    self.assertEqual(type(basic).ActiveProcessLimit.offset, 40)
                    limits.append((handle, kind, basic.LimitFlags, basic.ActiveProcessLimit))
                    return 1

                kernel.SetInformationJobObject.side_effect = set_information
                with mock.patch.object(a.ctypes, "WinDLL", return_value=kernel, create=True):
                    job = a.WindowsJob(single_process=single)
                    job.close()
                self.assertEqual(limits, [(7, 9, 0x2008 if single else 0x2000, 1 if single else 0)])
                kernel.CloseHandle.assert_called_once_with(7)


class WindowsGuardAccounting(unittest.TestCase):
    """Exercise the real Job query, runner and probe with Win32 ABI byte fixtures."""
    def exercise(self, counts=(2, 0, 1), *, selfcheck=True, denied=True,
                 query_ok=True, returned_size=48, payload=None, returncode=0,
                 proof_changes=None, missing=(), marker_when=None, cleanup_failure=None, child_error=None):
        self.events, self.diagnostics = [], io.StringIO()
        kernel, ntdll = mock.Mock(), mock.Mock()
        kernel.CreateJobObjectW.return_value = 7
        kernel.SetInformationJobObject.return_value = 1
        marker = None

        def close(handle):
            self.events.append("close-job")
            if marker_when == "close":
                marker.write_bytes(b"late child execution")
            return int(cleanup_failure != "close")

        kernel.CloseHandle.side_effect = close
        kernel.AssignProcessToJobObject.side_effect = lambda *args: self.events.append("assign") or 1
        ntdll.NtResumeProcess.side_effect = lambda *args: self.events.append("resume") or 0

        def query(handle, kind, pointer, size, returned):
            self.events.append("query")
            self.assertEqual((handle, kind, size), (7, 1, 48))
            self.assertIsNotNone(returned)
            # SDK layout: four LARGE_INTEGERs followed by four 32-bit DWORDs.
            ctypes.memmove(pointer, struct.pack("<4q4I", 0, 0, 0, 0, 0, *counts), 48)
            returned._obj.value = returned_size
            return int(query_ok)

        kernel.QueryInformationJobObject.side_effect = query
        process = mock.Mock(pid=123, _handle=456, returncode=returncode)
        process.poll.return_value = returncode

        def wait(timeout):
            self.events.append(("wait", timeout))
            if timeout == 5:
                if marker_when == "reap":
                    marker.write_bytes(b"late child execution")
                if marker_when == "remove-directory":
                    marker.parent.rmdir()
                if cleanup_failure == "reap":
                    raise OSError("owned reap failed")
            return returncode

        process.wait.side_effect = wait

        def popen(argv, **kwargs):
            nonlocal marker
            self.assertEqual(kwargs["creationflags"], 0x4 | 0x08000000)
            self.assertEqual(kwargs["stdin"], a.subprocess.DEVNULL)
            self.events.append("popen-suspended")
            if selfcheck:
                marker = Path(argv[3])
                self.assertFalse(marker.exists())
                self.assertTrue(marker.parent.is_dir())
                output = io.StringIO()
                denial = child_error if child_error is not None else OSError(13, "fixture denial; not a native Windows errno claim")

                def run_child(child_argv, *, check, timeout):
                    self.assertEqual(child_argv, [a.sys.executable, "-c",
                                                "import sys; f=open(sys.argv[1], 'xb'); f.write(b'child executed'); f.close()", str(marker)])
                    self.assertIs(check, True)
                    self.assertEqual(timeout, 5)
                    if marker_when == "attempt":
                        # Real child payload, without spawning any OS process.
                        with mock.patch.object(a.sys, "argv", ["-c", str(marker)]):
                            exec(child_argv[2], {})
                    if denied:
                        raise denial

                # Execute the actual inert probe, replacing only its child syscall.
                with mock.patch.object(a.sys, "platform", "win32"), \
                        mock.patch.object(a.sys, "argv", ["-c", str(marker)]), \
                        mock.patch.object(a.subprocess, "run", side_effect=run_child) as run, \
                        contextlib.redirect_stdout(output):
                    exec(argv[2], {})
                self.assertEqual(run.call_count, 1)
                data = output.getvalue()
                proof = json.loads(data)
                self.assertEqual(proof["childError"], {"errno": denial.errno, "winerror": getattr(denial, "winerror", None)} if denied else None)
                proof.update(proof_changes or {})
                for key in missing:
                    del proof[key]
                data = json.dumps(proof)
            else:
                data = json.dumps({"ok": True})
            kwargs["stdout"].write((data if payload is None else payload).encode())
            kwargs["stdout"].flush()
            return process

        try:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with mock.patch.object(a, "os", SimpleNamespace(name="nt")), \
                        mock.patch.object(a.ctypes, "WinDLL", side_effect=lambda name, **kw: kernel if name == "kernel32" else ntdll, create=True), \
                        mock.patch.object(a.subprocess, "Popen", side_effect=popen), \
                        contextlib.redirect_stdout(self.diagnostics):
                    if selfcheck:
                        proof = a.guard_selfcheck(root, {}, root)
                        self.assertFalse(marker.parent.exists())
                        return proof
                    # A label must never opt a real CLI call into probe accounting.
                    return a.guarded_run(["inert-cli"], root, {}, root, "guard-selfcheck")
        finally:
            self.assertEqual(self.events[:4], ["popen-suspended", "assign", "resume", ("wait", 30)])
            self.assertEqual(self.events[-2:], ["close-job", ("wait", 5)])
            kernel.TerminateJobObject.assert_not_called()
            process.kill.assert_not_called()

    def test_observed_native_denial_with_zero_limit_terminations(self):
        proof = self.exercise((2, 1, 0))
        self.assertIs(proof["childDenied"], True)
        self.assertEqual(proof["jobAccounting"], {"TotalProcesses": 2, "ActiveProcesses": 1,
                                                  "TotalTerminatedProcesses": 0})

    def test_documented_denied_attempt_passes_only_selfcheck_with_counters(self):
        for active in (0, 1, 2):
            for terminated in (0, 1):
                with self.subTest(active=active, terminated=terminated):
                    proof = self.exercise((2, active, terminated))
                    self.assertIs(proof["childDenied"], True)
                    self.assertIs(proof["childExecutionAbsent"], True)
                    self.assertEqual(proof["childAttempts"], 1)
                    self.assertEqual(proof["jobAccounting"], {"TotalProcesses": 2, "ActiveProcesses": active,
                                                              "TotalTerminatedProcesses": terminated})

    def test_successful_child_creation_cannot_pass_even_with_denial_counters(self):
        for counts in ((2, 0, 0), (2, 0, 1)):
            with self.subTest(counts=counts), self.assertRaisesRegex(ValueError, "Child guard ineffective.*TotalProcesses"):
                self.exercise(counts, denied=False)

    def test_cli_keeps_strict_total_one_even_for_limit_terminated_processes(self):
        self.assertEqual(self.exercise((1, 0, 0), selfcheck=False), {"ok": True})
        for counts in ((2, 0, 0), (2, 1, 1), (3, 0, 2)):
            with self.subTest(counts=counts), self.assertRaisesRegex(ValueError, "CLI spawned additional Job processes.*TotalProcesses"):
                self.exercise(counts, selfcheck=False)

    def test_probe_requires_exactly_one_attempt_and_no_extra_termination(self):
        for counts in ((1, 0, 0), (2, 0, 2), (3, 0, 1), (3, 0, 2)):
            with self.subTest(counts=counts), self.assertRaisesRegex(ValueError, "Self-check Job accounting.*TotalProcesses"):
                self.exercise(counts)

    def test_marker_execution_before_or_during_cleanup_cannot_pass_forged_denial(self):
        for counts in ((2, 1, 0), (2, 0, 1)):
            for marker_when in ("attempt", "close", "reap"):
                with self.subTest(counts=counts, marker_when=marker_when), \
                        self.assertRaisesRegex(ValueError, "Child execution marker found after cleanup.*TotalProcesses"):
                    self.exercise(counts, marker_when=marker_when)
        with self.assertRaisesRegex(ValueError, "Child execution marker found after cleanup"):
            self.exercise(denied=False, marker_when="attempt",
                          proof_changes={"childDenied": True, "childError": {"errno": 13, "winerror": None}})
        with self.assertRaisesRegex(ValueError, "Child marker directory missing after cleanup"):
            self.exercise(marker_when="remove-directory")

    def test_missing_or_malformed_required_proof_cannot_pass(self):
        for key in ("childDenied", "childAttempts", "childMarker", "childError"):
            with self.subTest(missing=key), self.assertRaisesRegex(ValueError, "TotalProcesses"):
                self.exercise(missing=(key,))
        for changes in ({"childAttempts": 0}, {"childAttempts": 2}, {"childAttempts": True},
                        {"childMarker": "/wrong/marker"}, {"childMarker": None},
                        {"childError": None}, {"childError": {}}, {"childError": "OSError"},
                        {"childError": {"errno": True, "winerror": None}},
                        {"childError": {"errno": 13, "winerror": "guessed"}}):
            with self.subTest(changes=changes), self.assertRaisesRegex(ValueError, "Invalid child.*proof.*TotalProcesses"):
                self.exercise(proof_changes=changes)

    def test_successful_probe_cannot_hide_cleanup_failure(self):
        for failure in ("close", "reap"):
            with self.subTest(failure=failure), self.assertRaisesRegex(RuntimeError, "Owned process cleanup"):
                self.exercise(cleanup_failure=failure)

    def test_oserror_codes_are_captured_without_assuming_native_values(self):
        for errno, winerror in ((13, None), (22, 12345), (None, None)):
            with self.subTest(errno=errno, winerror=winerror):
                error = OSError(errno, "fixture message must not be logged")
                error.winerror = winerror  # Synthetic values, not native Windows evidence.
                proof = self.exercise(child_error=error)
                self.assertEqual(proof["childError"], {"errno": errno, "winerror": winerror})
                self.assertIn("Guard child OSError: " + json.dumps(proof["childError"], sort_keys=True), self.diagnostics.getvalue())
                self.assertNotIn("fixture message", self.diagnostics.getvalue())

    def test_invalid_accounting_and_query_failures_fail_closed_and_cleanup(self):
        for counts in ((0, 0, 0), (2, 3, 1), (2, 0, 3)):
            with self.subTest(counts=counts), self.assertRaisesRegex(ValueError, "Invalid Job accounting.*TotalProcesses"):
                self.exercise(counts)
        with self.assertRaisesRegex(ValueError, "Invalid Job accounting.*TotalTerminatedProcesses"):
            self.exercise((1, 0, 1), selfcheck=False)
        for kwargs in ({"query_ok": False}, {"returned_size": 0}, {"returned_size": 47}, {"returned_size": 64}):
            with self.subTest(kwargs=kwargs), self.assertRaisesRegex(ValueError, "Job accounting"):
                self.exercise(**kwargs)

    def test_bad_probe_output_cannot_hide_counts_or_skip_cleanup(self):
        for payload in ('{}', '[]', '{"childDenied": 1}', 'not-json'):
            with self.subTest(payload=payload), self.assertRaisesRegex(ValueError, "TotalProcesses"):
                self.exercise(payload=payload)
            self.assertIn('"TotalProcesses": 2', self.diagnostics.getvalue())

    def test_failed_probe_exit_retains_counts_and_cleanup(self):
        with self.assertRaisesRegex(ValueError, "expected 0, got 1.*TotalProcesses"):
            self.exercise(returncode=1)


@unittest.skipUnless(a.os.name == "nt", "Native Windows Job Object probe")
class NativeWindowsGuard(unittest.TestCase):
    def test_inert_guard_before_release_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("data", "config", "tmp"):
                (root / name).mkdir()
            proof = a.guard_selfcheck(root, a.cli_environment(root), root)
            self.assertIs(proof["childDenied"], True)
            self.assertIs(proof["childExecutionAbsent"], True)
            self.assertEqual(proof["childAttempts"], 1)
            self.assertEqual(set(proof["childError"]), {"errno", "winerror"})
            self.assertEqual(proof["jobAccounting"]["TotalProcesses"], 2)
            self.assertIn(proof["jobAccounting"]["TotalTerminatedProcesses"], (0, 1))


if __name__ == "__main__":
    unittest.main()
