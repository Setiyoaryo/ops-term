"""Portability package checks (no Rust build required)."""
import importlib.util
import io
import os
import hashlib
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('linux_distribution', Path(__file__).with_name('linux_distribution.py'))
distribution = importlib.util.module_from_spec(SPEC) if SPEC else None
if SPEC and SPEC.loader and SPEC.origin and Path(SPEC.origin).exists():
    SPEC.loader.exec_module(distribution)


class ElfTests(unittest.TestCase):
    def test_glibc_baseline(self):
        self.assertTrue(hasattr(distribution, 'validate_elf_metadata'), 'ELF validator must exist')
        distribution.validate_elf_metadata('Name: GLIBC_2.9 Name: GLIBC_2.35', '(NEEDED) Shared library: [libc.so.6]', 'x86_64')
        with self.assertRaisesRegex(ValueError, 'GLIBC_2.39'):
            distribution.validate_elf_metadata('Name: GLIBC_2.39', '', 'x86_64')

    def test_dynamic_dependencies_and_search_paths(self):
        self.assertTrue(hasattr(distribution, 'validate_elf_metadata'), 'ELF validator must exist')
        for dynamic in ('(NEEDED) Shared library: [libdbus-1.so.3]', '(NEEDED) Shared library: [libsystemd.so.0]', '(RUNPATH) Library runpath: [/tmp/lib]'):
            with self.subTest(dynamic=dynamic), self.assertRaises(ValueError):
                distribution.validate_elf_metadata('GLIBC_2.35', dynamic, 'aarch64')

    def test_rejects_newer_glibc_abi_requirements(self):
        with self.assertRaisesRegex(ValueError, 'GLIBC_ABI_DT_RELR'):
            distribution.validate_elf_metadata('Name: GLIBC_ABI_DT_RELR', '', 'x86_64')

    def test_architecture_validation(self):
        self.assertTrue(hasattr(distribution, 'validate_elf_header'), 'ELF header validator must exist')
        distribution.validate_elf_header('Class: ELF64\nMachine: Advanced Micro Devices X86-64', 'x86_64')
        with self.assertRaises(ValueError):
            distribution.validate_elf_header('Class: ELF64\nMachine: AArch64', 'x86_64')


class PackagingTests(unittest.TestCase):
    def test_archive_and_installer_preserve_arguments(self):
        self.assertTrue(hasattr(distribution, 'build_distribution'), 'distribution builder must exist')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'binary'
            binary.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
            binary.chmod(0o755)
            def fake_appimage(appdir, target, arch):
                self.assertEqual((appdir / 'org.opsterm.app.desktop').read_text().count('Exec=opsterm-egui'), 1)
                self.assertTrue((appdir / 'AppRun').stat().st_mode & 0o111)
                self.assertTrue((appdir / 'org.opsterm.app.png').exists())
                self.assertIn('AFL-2.1', (appdir / 'usr/share/licenses/opsterm/THIRD_PARTY_NOTICES.txt').read_text())
                self.assertIn('Academic Free License', (appdir / 'usr/share/licenses/opsterm/dbus-COPYING').read_text())
                target.write_bytes(b'image')
            with patch.object(distribution, 'validate_elf'), patch.object(distribution, 'build_appimage', side_effect=fake_appimage):
                artifacts = distribution.build_distribution(binary, '1.2.3', 'x86_64', root / 'out')
            archive = next(path for path in artifacts if path.name.endswith('.tar.gz'))
            with tarfile.open(archive) as tar:
                # Every member is generated locally; avoid Python 3.12's default warning.
                import inspect
                kwargs = {'filter': 'data'} if 'filter' in inspect.signature(tar.extractall).parameters else {}
                tar.extractall(root / 'extracted', **kwargs)
            package = next((root / 'extracted').iterdir())
            prefix = root / 'prefix with spaces $cash %field `literal`'
            result = subprocess.run([str(package / 'install.sh'), '--prefix', str(prefix)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            installed = prefix / 'bin/opsterm-egui'
            self.assertEqual(subprocess.check_output([str(installed), 'space arg', '$(false)'], text=True), 'space arg\n$(false)\n')
            desktop = (prefix / 'share/applications/org.opsterm.app.desktop').read_text()
            self.assertIn(r'prefix with spaces \\$cash %%field \\`literal\\`/bin/opsterm-egui', desktop)
            self.assertTrue((package / 'diagnose.sh').stat().st_mode & 0o111)
            self.assertTrue((package / 'LICENSE').exists())
            self.assertIn('AFL-2.1', (package / 'THIRD_PARTY_NOTICES.txt').read_text())
            self.assertIn('Academic Free License', (package / 'dbus-COPYING').read_text())
            for arguments in (['--prefix', 'relative'], ['--prefix'], ['--unknown'], ['--prefix', '/tmp/bad\npath']):
                rejected = subprocess.run([str(package / 'install.sh'), *arguments], capture_output=True, text=True)
                self.assertNotEqual(rejected.returncode, 0, arguments)

    def test_download_rejects_bad_digest_before_execution(self):
        self.assertTrue(hasattr(distribution, 'download_verified'), 'verified downloader must exist')
        with tempfile.TemporaryDirectory() as temporary, patch.object(distribution.urllib.request, 'urlopen', return_value=io.BytesIO(b'bad')):
            target = Path(temporary) / 'tool'
            with self.assertRaisesRegex(ValueError, 'SHA-256'):
                distribution.download_verified('https://github.com/AppImage/example', '0' * 64, target)
            self.assertFalse(target.exists())

    def test_appimage_tool_and_runtime_are_verified_before_execution(self):
        calls = []
        def download(url, digest, target):
            calls.append(('download', url, digest))
            target.write_bytes(b'verified')
        def run(argv, **kwargs):
            calls.append(('run', argv))
            if '--runtime-file' in argv:
                Path(argv[-1]).write_bytes(b'image')
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(distribution, 'download_verified', side_effect=download), patch.object(distribution.subprocess, 'run', side_effect=run):
                distribution.build_appimage(root / 'AppDir', root / 'output.AppImage', 'x86_64')
        self.assertEqual([entry[0] for entry in calls], ['download', 'download', 'run', 'run'])
        self.assertIn('/1.9.1/', calls[0][1])
        self.assertEqual(calls[0][2], distribution.APPIMAGETOOL_SHA256['x86_64'])
        self.assertIn('/20251108/', calls[1][1])
        self.assertEqual(calls[1][2], distribution.RUNTIME_SHA256['x86_64'])
        self.assertIn('--appimage-extract', calls[2][1])
        self.assertIn('--runtime-file', calls[3][1])
        self.assertIn('--no-appstream', calls[3][1])

    def test_verified_download_publishes_exact_content(self):
        content = b'verified tool'
        with tempfile.TemporaryDirectory() as temporary, patch.object(distribution.urllib.request, 'urlopen', return_value=io.BytesIO(content)):
            target = Path(temporary) / 'tool'
            distribution.download_verified('https://github.com/AppImage/example', hashlib.sha256(content).hexdigest(), target)
            self.assertEqual(target.read_bytes(), content)
            self.assertFalse(target.with_name('tool.download').exists())

    def test_rejects_path_components(self):
        self.assertTrue(hasattr(distribution, 'build_distribution'), 'distribution builder must exist')
        for version in ('../escape', 'x/other', 'a\nExec=bad'):
            with self.assertRaises(ValueError):
                distribution.build_distribution(Path('/missing'), version, 'x86_64', Path('/tmp/not-created'))


if __name__ == '__main__':
    unittest.main()
