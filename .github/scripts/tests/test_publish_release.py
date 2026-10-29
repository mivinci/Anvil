"""Offline coverage for release lookup at the draft/public boundary."""

import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import publish_release


class ReleaseLookup(unittest.TestCase):
    def setUp(self):
        self.environment = patch.dict('os.environ', {'GITHUB_REPOSITORY': 'owner/repo'})
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def response(self, code=0, data=None, error=''):
        return subprocess.CompletedProcess([], code, json.dumps(data), error)

    def test_public_release_uses_tag_endpoint(self):
        public = {'id': 1, 'tag_name': 'desktop/v1.0.0', 'draft': False}
        with patch.object(publish_release.subprocess, 'run', return_value=self.response(data=public)), \
                patch.object(publish_release, 'gh') as listing:
            self.assertEqual(publish_release.release('desktop/v1.0.0'), public)
            listing.assert_not_called()

    def test_draft_on_later_page_is_found(self):
        draft = {'id': 2, 'tag_name': 'desktop/v1.0.0', 'draft': True}
        full_page = [{'tag_name': f'other/{number}'} for number in range(100)]
        with patch.object(publish_release.subprocess, 'run', return_value=self.response(1, error='HTTP (HTTP 404)')), \
                patch.object(publish_release, 'gh', side_effect=[json.dumps(full_page), json.dumps([draft])]) as listing:
            self.assertEqual(publish_release.release('desktop/v1.0.0'), draft)
            self.assertEqual(listing.call_args_list[1].args,
                             ('api', 'repos/owner/repo/releases?per_page=100&page=2'))

    def test_absent_release_requires_successful_empty_listing(self):
        with patch.object(publish_release.subprocess, 'run', return_value=self.response(1, error='HTTP (HTTP 404)')), \
                patch.object(publish_release, 'gh', return_value='[]'):
            self.assertIsNone(publish_release.release('desktop/v1.0.0'))

    def test_permission_failure_is_not_treated_as_absence(self):
        with patch.object(publish_release.subprocess, 'run', return_value=self.response(1, error='HTTP (HTTP 403)')), \
                patch.object(publish_release, 'gh') as listing:
            with self.assertRaises(RuntimeError):
                publish_release.release('desktop/v1.0.0')
            listing.assert_not_called()

    def test_list_failure_is_not_treated_as_absence(self):
        with patch.object(publish_release.subprocess, 'run', return_value=self.response(1, error='HTTP (HTTP 404)')), \
                patch.object(publish_release, 'gh', side_effect=RuntimeError('network failure')):
            with self.assertRaisesRegex(RuntimeError, 'network failure'):
                publish_release.release('desktop/v1.0.0')


class ReleaseResponseIdentity(unittest.TestCase):
    def setUp(self):
        context = patch.dict('os.environ', {'GITHUB_REPOSITORY': 'owner/repo'})
        context.start()
        self.addCleanup(context.stop)

    def test_create_uses_response_id_without_tag_lookup(self):
        info = {'id': 42, 'tag_name': 'runtime/v26.1.0', 'draft': True, 'assets': []}
        with patch.object(publish_release, 'gh', return_value=json.dumps(info)) as api, \
             patch.object(publish_release, 'release') as lookup:
            self.assertEqual(publish_release.create_draft('runtime/v26.1.0', 'abc', 'title'), info)
            lookup.assert_not_called()
            self.assertIn('tag_name=runtime/v26.1.0', api.call_args.args)
            self.assertIn('target_commitish=abc', api.call_args.args)

    def test_create_rejects_wrong_response_identity(self):
        for info in ({'id': 42, 'tag_name': 'wrong', 'draft': True},
                     {'id': 42, 'tag_name': 'runtime/v26.1.0', 'draft': False},
                     {'tag_name': 'runtime/v26.1.0', 'draft': True}):
            with patch.object(publish_release, 'gh', return_value=json.dumps(info)), \
                 self.assertRaises(ValueError):
                publish_release.create_draft('runtime/v26.1.0', 'abc', 'title')

    def test_readback_uses_id_and_rejects_changed_identity(self):
        info = {'id': 42, 'tag_name': 'runtime/v26.1.0'}
        with patch.object(publish_release, 'gh', return_value=json.dumps(info)) as api:
            self.assertEqual(publish_release.release_by_id(info), info)
            api.assert_called_once_with('api', 'repos/owner/repo/releases/42')
        with patch.object(publish_release, 'gh', return_value=json.dumps({**info, 'id': 43})), \
             self.assertRaises(ValueError):
            publish_release.release_by_id(info)

    def test_download_uses_asset_id_not_tag_resolution(self):
        import tempfile
        info = {'tag_name': 'runtime/v26.1.0', 'assets': [{'id': 12, 'name': 'payload'}]}
        def fetch(*args, **kwargs):
            kwargs['stdout'].write(b'payload bytes')
        with tempfile.TemporaryDirectory() as directory, \
             patch.object(publish_release.subprocess, 'run', side_effect=fetch) as fetcher, \
             patch.object(publish_release, 'gh') as gh:
            publish_release.download(info, Path(directory), {'payload'})
            self.assertEqual((Path(directory) / 'payload').read_bytes(), b'payload bytes')
            self.assertIn('repos/owner/repo/releases/assets/12', fetcher.call_args.args[0])
            gh.assert_not_called()


class PublicRecovery(unittest.TestCase):
    def test_public_version_only_downloads_and_reconciles_feed(self):
        import hashlib
        info = {'id': 42, 'tag_name': 'runtime/v26.1.0', 'draft': False}
        def download(info, folder, names):
            checksums = []
            for name in sorted(names - {'SHA256SUMS'}):
                (folder / name).write_bytes(b'published bytes')
                checksums.append(f'{hashlib.sha256(b"published bytes").hexdigest()}  {name}\n')
            (folder / 'SHA256SUMS').write_text(''.join(checksums))
        with patch.dict('os.environ', RELEASE_TAG='runtime/v26.1.0', RELEASE_SHA='abc'), \
             patch.object(sys, 'argv', ['publish_release.py', 'publish', '--product', 'runtime']), \
             patch.object(publish_release.subprocess, 'check_output', return_value='abc'), \
             patch.object(publish_release, 'validate_release_checkout'), \
             patch.object(publish_release, 'release', return_value=info), \
             patch.object(publish_release, 'release_by_id', return_value=info), \
             patch.object(publish_release, 'verify_identity'), \
             patch.object(publish_release, 'expected', return_value={'binary'}), \
             patch.object(publish_release, 'download', side_effect=download), \
             patch.object(publish_release, 'validate_runtime_index'), \
             patch.object(publish_release, 'update_feed') as feed, \
             patch.object(publish_release, 'report'), \
             patch.object(publish_release, 'gh') as mutation, \
             patch.object(publish_release, 'create_draft') as create, \
             patch.object(publish_release, 'runtime_index') as rebuild:
            publish_release.main()
            mutation.assert_not_called()
            create.assert_not_called()
            rebuild.assert_not_called()
            self.assertEqual(feed.call_args.args[0], 'runtime-updates')


if __name__ == '__main__':
    unittest.main()
