"""Reject mixed-architecture runtime reuse before overwriting a valid runtime."""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
from runtime_target import TARGETS, validate_elf


def elf_header(target):
    header = bytearray(20)
    header[:6] = b"\x7fELF\x02\x01"
    header[18:20] = TARGETS[target]["elf_machine"].to_bytes(2, "little")
    return bytes(header)


def archive_bytes(files):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as archive:
        for name, data in files.items():
            entry = tarfile.TarInfo(name)
            entry.size = len(data)
            entry.mode = 0o755
            archive.addfile(entry, io.BytesIO(data))
    return output.getvalue()


class RuntimeReuse(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        (self.root / "scripts").mkdir()
        for name in ("refresh_runtime.py", "runtime_target.py"):
            shutil.copy2(SCRIPTS / name, self.root / "scripts" / name)
        (self.root / "engine").mkdir()
        (self.root / "engine/worker.py").write_text("# new worker\n")
        (self.root / "vendor/modeltrace").mkdir(parents=True)
        (self.root / "runtime").mkdir()
        self.destination = self.root / "runtime/runtime.tar.gz"
        self.destination.write_bytes(b"keep existing runtime on failure")

    def package(self, target, payload_target=None):
        spec = TARGETS[target]
        header = elf_header(payload_target or target)
        files = {
            "bin/modeltrace-health": header,
            "runtime/runtime.tar.gz": archive_bytes({
                "usr/local/bin/python3.11": header,
                f"usr/lib/{spec['libdir']}/{spec['loader']}": header,
                "usr/local/lib/python3.11/site-packages/numpy/_core.so": header,
            }),
        }
        manifest = {"version": "test", "package": {
            "target": {"os": "linux", "architecture": target.split("-")[0]},
            "files": {name: hashlib.sha256(data).hexdigest() for name, data in files.items()},
        }}
        files["plugin.json"] = json.dumps(manifest).encode()
        package = self.root / "previous.tar.gz"
        package.write_bytes(archive_bytes(files))
        package.with_name(package.name + ".sha256").write_text(hashlib.sha256(package.read_bytes()).hexdigest())
        return package

    def refresh(self, target, package):
        env = os.environ.copy()
        env["CPR_TARGET"] = target
        return subprocess.run([sys.executable, str(self.root / "scripts/refresh_runtime.py"), str(package)],
                              env=env, text=True, capture_output=True)

    def test_matching_targets_refresh_worker(self):
        for target in TARGETS:
            with self.subTest(target=target):
                result = self.refresh(target, self.package(target))
                self.assertEqual(result.returncode, 0, result.stderr)
                with tarfile.open(self.destination) as archive:
                    self.assertEqual(archive.extractfile("engine/worker.py").read(), b"# new worker\n")
                    validate_elf(archive.extractfile("usr/local/bin/python3.11").read(), target)

    def test_wrong_target_preserves_existing_runtime(self):
        targets = list(TARGETS)
        result = self.refresh(targets[0], self.package(targets[1]))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Source package target does not match", result.stderr)
        self.assertEqual(self.destination.read_bytes(), b"keep existing runtime on failure")

    def test_manifest_cannot_hide_wrong_elf(self):
        targets = list(TARGETS)
        result = self.refresh(targets[0], self.package(targets[0], targets[1]))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("ELF architecture does not match", result.stderr)
        self.assertEqual(self.destination.read_bytes(), b"keep existing runtime on failure")

    def test_changed_package_is_rejected(self):
        target = next(iter(TARGETS))
        package = self.package(target)
        package.write_bytes(package.read_bytes() + b"changed")
        result = self.refresh(target, package)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Source artifact digest mismatch", result.stderr)
        self.assertEqual(self.destination.read_bytes(), b"keep existing runtime on failure")

    def test_unsupported_elf_format_is_rejected(self):
        target = next(iter(TARGETS))
        for header in (b"not ELF", b"\x7fELF\x01\x01" + bytes(14), b"\x7fELF\x02\x02" + bytes(14)):
            with self.subTest(header=header):
                with self.assertRaisesRegex(ValueError, "64-bit little-endian"):
                    validate_elf(header, target)


if __name__ == "__main__":
    unittest.main()
