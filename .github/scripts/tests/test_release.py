"""Offline release identity checks; never creates tags or contacts GitHub."""
import sys
from pathlib import Path
import unittest
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release
import publish_release


class RuntimeMigrationTests(unittest.TestCase):
    def test_both_tag_namespaces_share_version_sequence(self):
        def command(*args):
            return {'runtime/v*': 'runtime/v26.0.0-alpha.3',
                    'cli/v*': 'cli/v26.0.0-alpha.2'}[args[-1]]
        with patch.object(release, 'command', side_effect=command):
            tags = dict(release.tag_versions('runtime'))
        self.assertEqual(set(tags), {'runtime/v26.0.0-alpha.3', 'cli/v26.0.0-alpha.2'})
        self.assertGreater(tags['runtime/v26.0.0-alpha.3'], tags['cli/v26.0.0-alpha.2'])

    def test_preparation_rejects_higher_legacy_version(self):
        with patch.object(release, 'product_version', return_value='26.0.0-alpha.1'), \
             patch.object(release, 'tag_versions', return_value=[
                 ('cli/v26.0.0-alpha.3', release.version_key('26.0.0-alpha.3'))]):
            with self.assertRaisesRegex(ValueError, 'existing product tag cli/v'):
                release.new_version('runtime', '26.0.0-alpha.2')

    def test_resolve_rejects_legacy_same_version_before_tag_creation(self):
        with patch.object(release, 'github_context', return_value=('owner/repo', 'abc')), \
             patch.object(release, 'product_version', return_value='26.0.0-alpha.2'), \
             patch.object(release, 'tag_versions', return_value=[
                 ('cli/v26.0.0-alpha.2', release.version_key('26.0.0-alpha.2'))]):
            with self.assertRaisesRegex(ValueError, 'superseded by product tag cli/v'):
                release.resolve('runtime')

    def test_desktop_only_reads_desktop_tags(self):
        with patch.object(release, 'command', return_value='desktop/v26.0.0-alpha.2') as command:
            release.tag_versions('desktop')
            command.assert_called_once_with('git', 'tag', '--list', 'desktop/v*')

    def test_runtime_preserves_binary_asset_names(self):
        names = publish_release.expected('runtime', '26.0.0-alpha.3')
        self.assertEqual(len(names), 8)
        self.assertTrue(all(name.startswith('anvil-26.0.0-alpha.3-') for name in names))


class JointPreparationTests(unittest.TestCase):
    def test_joint_plan_keeps_independent_versions_and_exact_files(self):
        products, changes = release.preparation_plan({
            'desktop': '99.0.0-alpha.3', 'runtime': '99.0.0-alpha.4'})
        self.assertEqual(products['desktop']['version'], '99.0.0-alpha.3')
        self.assertEqual(products['runtime']['version'], '99.0.0-alpha.4')
        self.assertEqual(set(changes), {release.TAURI, Path('Cargo.toml'), Path('Cargo.lock'),
            *(p.relative_to(release.ROOT) for p in (release.ROOT / release.MANIFESTS).glob('*.json'))})
        self.assertIn('99.0.0-alpha.4', changes[Path('Cargo.toml')])
        self.assertIn('99.0.0-alpha.3', changes[release.TAURI])

    def test_single_product_omits_other_product_files(self):
        products, changes = release.preparation_plan({'runtime': '99.0.0-alpha.3'})
        self.assertEqual(set(products), {'runtime'})
        self.assertEqual(set(changes), {Path('Cargo.toml'), Path('Cargo.lock')})

    def test_empty_or_unknown_product_rejected(self):
        for versions in ({}, {'cli': '99.0.0'}):
            with self.assertRaises(ValueError):
                release.preparation_plan(versions)

    def test_invalid_second_product_does_not_create_branch_or_write(self):
        with patch.object(release, 'github_context', return_value=('owner/repo', 'base')), \
             patch.object(release, 'command', return_value='base'), \
             patch.object(release, 'run') as run:
            with self.assertRaises(ValueError):
                release.prepare({'desktop': '99.0.0', 'runtime': 'invalid'})
            run.assert_not_called()

    def test_joint_prepare_creates_one_pr_and_dispatches_ci_once(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            captured, calls = [], []
            def command(*args):
                calls.append(args)
                if args[:3] == ('git', 'rev-parse', 'origin/main'):
                    return 'base'
                if args[:3] == ('git', 'diff', '--name-only'):
                    return 'desktop.json\nruntime.toml'
                if args[:3] == ('gh', 'pr', 'create'):
                    captured.append(Path(args[args.index('--body-file') + 1]).read_text())
                    return 'https://github.com/owner/repo/pull/1'
                return 'head'
            products = {'desktop': {'current': '26.0.0', 'version': '26.1.0'},
                        'runtime': {'current': '26.0.0', 'version': '26.2.0'}}
            changes = {Path('desktop.json'): 'desktop', Path('runtime.toml'): 'runtime'}
            def run(*args, **kwargs):
                calls.append(args)
                return SimpleNamespace(returncode=2 if args[1] == 'ls-remote' else
                                       1 if args[1] == 'show-ref' else 0)
            with patch.object(release, 'ROOT', root), \
                 patch.object(release, 'github_context', return_value=('owner/repo', 'base')), \
                 patch.object(release, 'preparation_plan', return_value=(products, changes)), \
                 patch.object(release, 'product_version', side_effect=lambda p: products[p]['version']), \
                 patch.object(release, 'command', side_effect=command), \
                 patch.object(release, 'run', side_effect=run), \
                 patch.object(release, 'summary'), patch.object(release, 'write_outputs'):
                release.prepare({'desktop': '26.1.0', 'runtime': '26.2.0'}, 'A user-facing change')
            self.assertEqual(len(captured), 1)
            self.assertIn('| desktop | 26.0.0 | 26.1.0 |', captured[0])
            self.assertIn('| runtime | 26.0.0 | 26.2.0 |', captured[0])
            self.assertIn('A user-facing change', captured[0])
            self.assertEqual(sum(c[:3] == ('gh', 'workflow', 'run') for c in calls), 1)
            self.assertEqual(sum(c[:2] == ('git', 'commit') for c in calls), 1)


if __name__ == '__main__':
    unittest.main()
