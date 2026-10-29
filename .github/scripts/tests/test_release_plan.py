"""Offline automatic-release trust and version selection boundaries."""
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release
import release_plan


class CheckoutTrust(unittest.TestCase):
    def event(self, **changes):
        return {'head_sha': 'abc', 'head_branch': 'main', 'event': 'push',
                'status': 'completed', 'conclusion': 'success',
                'path': '.github/workflows/rust.yml',
                'repository': {'full_name': 'owner/repo'},
                'head_repository': {'full_name': 'owner/repo'}, **changes}

    def validate(self, event):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'event.json'
            path.write_text(json.dumps({'workflow_run': event}))
            with patch.dict(os.environ, GITHUB_ACTIONS='true', GITHUB_REF='refs/heads/main',
                            GITHUB_EVENT_NAME='workflow_run', GITHUB_EVENT_PATH=str(path),
                            GITHUB_REPOSITORY='owner/repo', GITHUB_SHA='newer-main'):
                release.validate_release_checkout('abc')

    def test_successful_exact_main_ci_allows_main_to_advance(self):
        self.validate(self.event())

    def test_untrusted_or_wrong_ci_is_rejected(self):
        for changes in ({'head_sha': 'other'}, {'head_branch': 'feature'}, {'event': 'pull_request'},
                        {'conclusion': 'failure'}, {'status': 'in_progress'},
                        {'path': '.github/workflows/other.yml'},
                        {'head_repository': {'full_name': 'fork/repo'}},
                        {'repository': {'full_name': 'other/repo'}}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.validate(self.event(**changes))

    def test_manual_retry_uses_explicit_sha_and_requires_main(self):
        with patch.dict(os.environ, GITHUB_ACTIONS='true', GITHUB_REF='refs/heads/main',
                        GITHUB_EVENT_NAME='workflow_dispatch', RELEASE_SHA='abc', GITHUB_SHA='new'):
            release.validate_release_checkout('abc')
            with self.assertRaises(ValueError):
                release.validate_release_checkout('new')
            with patch.dict(os.environ, GITHUB_REF='refs/heads/feature'), self.assertRaises(ValueError):
                release.validate_release_checkout('abc')


class Selection(unittest.TestCase):
    def changed(self, current):
        def command(*args):
            if args[:2] == ('git', 'rev-parse'):
                return 'parent'
            if str(release.TAURI) in args[-1]:
                return '{"version":"26.0.0"}'
            return '[workspace.package]\nversion="26.0.0"'
        with patch.object(release, 'command', side_effect=command), \
             patch.object(release, 'product_version', side_effect=lambda p: current[p]):
            return release_plan.changed_products()

    def test_ordinary_commit_does_not_publish(self):
        self.assertEqual(self.changed({'desktop': '26.0.0', 'runtime': '26.0.0'}), {})

    def test_single_or_joint_version_changes(self):
        self.assertEqual(self.changed({'desktop': '26.1.0', 'runtime': '26.0.0'}), {'desktop': '26.1.0'})
        self.assertEqual(self.changed({'desktop': '26.1.0', 'runtime': '26.2.0'}),
                         {'desktop': '26.1.0', 'runtime': '26.2.0'})

    def test_version_decrease_rejected(self):
        with self.assertRaises(ValueError):
            self.changed({'desktop': '25.0.0', 'runtime': '26.0.0'})

    def test_no_version_skips_approval_and_key_lookup(self):
        with patch.object(release, 'github_context', return_value=('owner/repo', 'abc')), \
             patch.object(release_plan, 'changed_products', return_value={}), \
             patch.object(release_plan, 'public_keys') as keys, \
             patch.object(release, 'write_outputs') as outputs, patch.object(release, 'summary'):
            release_plan.plan()
            outputs.assert_called_once_with(selected='false')
            keys.assert_not_called()

    def test_missing_key_stops_before_resolving_or_tagging(self):
        with patch.dict(os.environ, {'RUNTIME_PUBLIC_KEY': ''}), \
             patch.object(release, 'github_context', return_value=('owner/repo', 'abc')), \
             patch.object(release_plan, 'changed_products', return_value={'runtime': '26.1.0'}), \
             patch.object(release, 'resolve') as resolve, self.assertRaises(ValueError):
            release_plan.plan()
        resolve.assert_not_called()


class Approval(unittest.TestCase):
    def execute(self, bad_runtime=False):
        products = {'desktop': '26.1.0', 'runtime': '26.2.0'}
        calls = []
        def signing_key(seed):
            if bad_runtime and seed == bytes.fromhex('22' * 32):
                raise ValueError('invalid fixture key')
            return SimpleNamespace(verify_key=SimpleNamespace(encode=lambda: bytes.fromhex('33' * 32)))
        fake = SimpleNamespace(SigningKey=signing_key)
        def run(*args, **kwargs):
            calls.append(args)
            return SimpleNamespace(returncode=1 if args[1] == 'show-ref' else 0)
        with patch.dict(sys.modules, {'nacl.signing': fake}), \
             patch.dict(os.environ, DESKTOP_SIGNING_KEY='11' * 32, RUNTIME_SIGNING_KEY='22' * 32,
                        DESKTOP_PUBLIC_KEY='33' * 32, RUNTIME_PUBLIC_KEY='33' * 32), \
             patch.object(release, 'github_context', return_value=('owner/repo', 'abc')), \
             patch.object(release_plan, 'changed_products', return_value=products), \
             patch.object(release, 'resolve'), patch.object(release, 'summary'), \
             patch.object(release, 'run', side_effect=run):
            if bad_runtime:
                with self.assertRaises(ValueError):
                    release_plan.approve(products)
            else:
                release_plan.approve(products)
        return calls

    def test_all_keys_validated_before_any_tag_mutation(self):
        self.assertEqual(self.execute(bad_runtime=True), [])

    def test_joint_tags_pushed_atomically_at_same_sha(self):
        calls = self.execute()
        self.assertIn(('git', 'push', '--atomic', 'origin',
                       'refs/tags/desktop/v26.1.0', 'refs/tags/runtime/v26.2.0'), calls)
        tags = [c for c in calls if c[:2] == ('git', 'tag')]
        self.assertEqual(len(tags), 2)
        self.assertTrue(all(c[4] == 'abc' for c in tags))


if __name__ == '__main__':
    unittest.main()
