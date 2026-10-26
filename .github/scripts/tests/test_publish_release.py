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


if __name__ == '__main__':
    unittest.main()
