"""Retained artifact identity, selection and extraction boundaries, without network."""
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release_artifacts as artifacts

SHA = 'a' * 40
TAG = 'runtime/v26.0.0-alpha.3'
TARGET = 'x86_64-unknown-linux-gnu'


class RetainedArtifacts(unittest.TestCase):
    def run_data(self, **changes):
        return {'path': '.github/workflows/release.yml', 'head_branch': 'main',
                'event': 'workflow_dispatch', 'head_repository': {'full_name': 'owner/repo'}, **changes}

    def test_trust_rejects_fork_branch_and_other_workflow(self):
        self.assertTrue(artifacts.trusted(self.run_data(), 'owner/repo'))
        for change in ({'event': 'pull_request'}, {'head_branch': 'feature'},
                       {'path': '.github/workflows/rust.yml'},
                       {'head_repository': {'full_name': 'fork/repo'}}):
            self.assertFalse(artifacts.trusted(self.run_data(**change), 'owner/repo'))

    def test_identity_includes_product_version_sha_target(self):
        name = artifacts.identity('runtime', TAG, SHA, TARGET)
        for part in ('runtime', '26.0.0-alpha.3', SHA, TARGET):
            self.assertIn(part, name)
        with self.assertRaises(ValueError):
            artifacts.identity('runtime', 'cli/v26.0.0-alpha.3', SHA, TARGET)
        with self.assertRaises(ValueError):
            artifacts.identity('runtime', TAG, 'main', TARGET)

    def test_lookup_skips_expired_untrusted_and_missing_digest(self):
        name = artifacts.identity('runtime', TAG, SHA, TARGET)
        valid = {'name': name, 'expired': False, 'digest': 'sha256:' + 'b' * 64,
                 'id': 1, 'workflow_run': {'id': 9}}
        entries = [{**valid, 'expired': True}, {**valid, 'digest': ''},
                   {**valid, 'workflow_run': {'id': 8}}, valid]
        with patch.dict(os.environ, GITHUB_REPOSITORY='owner/repo'), \
             patch.object(artifacts, 'pages', side_effect=lambda path, _: entries if name in path else []), \
             patch.object(artifacts, 'api', side_effect=lambda path: self.run_data(
                 path='.github/workflows/rust.yml') if path.endswith('/8') else self.run_data()):
            self.assertEqual(artifacts.find('runtime', TAG, SHA), {TARGET: valid})

    def test_missing_targets_only_enter_build_matrix(self):
        with patch.dict(os.environ, RELEASE_TAG=TAG, RELEASE_SHA=SHA), \
             patch.object(sys, 'argv', ['release_artifacts.py', 'plan', '--product', 'runtime']), \
             patch.object(artifacts, 'find', return_value={TARGET: {'id': 1}}), \
             patch.object(artifacts, 'write_outputs') as outputs, patch.object(artifacts, 'summary'):
            artifacts.main()
        import json
        values = outputs.call_args.kwargs
        matrix = json.loads(values['matrix'])['include']
        self.assertEqual(len(matrix), 3)
        self.assertNotIn(TARGET, [item['target'] for item in matrix])
        self.assertEqual(values['build_required'], 'true')

    def test_all_retained_skips_build(self):
        with patch.dict(os.environ, RELEASE_TAG=TAG, RELEASE_SHA=SHA), \
             patch.object(sys, 'argv', ['release_artifacts.py', 'plan', '--product', 'runtime']), \
             patch.object(artifacts, 'find', return_value={t: {'id': 1} for t in artifacts.TARGETS}), \
             patch.object(artifacts, 'write_outputs') as outputs, patch.object(artifacts, 'summary'):
            artifacts.main()
        self.assertEqual(outputs.call_args.kwargs['build_required'], 'false')

    def test_payload_sets_match_global_publisher(self):
        for product in ('desktop', 'runtime'):
            for version in ('26.0.0', '26.0.0-alpha.3'):
                names = set()
                for target in artifacts.TARGETS:
                    subset = artifacts.target_files(product, version, target)
                    self.assertFalse(names & subset)
                    names |= subset
                self.assertEqual(names, artifacts.expected(product, version))

    def test_extract_checks_exact_files_and_crc(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'artifact.zip'
            output = root / 'out'
            output.mkdir()
            with zipfile.ZipFile(source, 'w') as archive:
                archive.writestr('anvil.bin', b'valid bytes')
            with self.assertRaises(ValueError):
                artifacts.extract(source, output, {'other.bin'})
            artifacts.extract(source, output, {'anvil.bin'})
            self.assertEqual((output / 'anvil.bin').read_bytes(), b'valid bytes')
            with self.assertRaises(FileExistsError):
                artifacts.extract(source, output, {'anvil.bin'})

    def test_extract_rejects_traversal_duplicate_and_oversize(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for filename in ('../escape', 'folder/file', 'a\\b'):
                source = root / 'artifact.zip'
                with zipfile.ZipFile(source, 'w') as archive:
                    archive.writestr(filename, b'x')
                with self.assertRaises(ValueError):
                    artifacts.extract(source, root, {filename})
            with zipfile.ZipFile(source, 'w') as archive:
                archive.writestr('a', b'12345')
            with patch.object(artifacts, 'MAX_BYTES', 4), self.assertRaises(ValueError):
                artifacts.extract(source, root, {'a'})

    def test_collect_rejects_archive_digest_mismatch(self):
        records = {t: {'id': i, 'digest': 'sha256:' + '0' * 64,
                       'workflow_run': {'id': 1}} for i, t in enumerate(artifacts.TARGETS)}
        def download(*args, **kwargs):
            kwargs['stdout'].write(b'incorrect archive')
        with tempfile.TemporaryDirectory() as directory, \
             patch.dict(os.environ, GITHUB_REPOSITORY='owner/repo'), \
             patch.object(artifacts, 'find', return_value=records), \
             patch.object(artifacts.subprocess, 'run', side_effect=download), \
             self.assertRaisesRegex(ValueError, 'digest mismatch'):
            artifacts.collect('runtime', TAG, SHA, Path(directory) / 'dist')

    def test_missing_artifact_stops_collection(self):
        with patch.object(artifacts, 'find', return_value={}), \
             patch.object(artifacts.subprocess, 'run') as download, self.assertRaises(ValueError):
            artifacts.collect('runtime', TAG, SHA, Path('unused'))
        download.assert_not_called()


if __name__ == '__main__':
    unittest.main()
