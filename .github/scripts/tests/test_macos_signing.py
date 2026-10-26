"""Signing orchestration tests; no real security commands or keychain access."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("macos_signing", Path(__file__).parents[1] / "macos_signing.py")
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)


class SigningTests(unittest.TestCase):
    def test_no_certificate_does_not_touch_keychains(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(signing, "run") as run:
            signing.setup()
            run.assert_not_called()

    def test_partial_configuration_fails(self):
        with patch.dict(os.environ, {"ANVIL_MACOS_CERTIFICATE_PASSWORD": "fixture"}, clear=True):
            with self.assertRaises(RuntimeError):
                signing.setup()

    def test_setup_exports_identity_and_removes_p12(self):
        with tempfile.TemporaryDirectory() as root:
            env = {"RUNNER_TEMP": root, "GITHUB_ENV": root + "/env", "GITHUB_STATE_FILE": root + "/state",
                   "ANVIL_MACOS_CERTIFICATE_P12": "Zml4dHVyZQ==", "ANVIL_MACOS_CERTIFICATE_PASSWORD": "fixture"}
            def run(args):
                if args[1] == "find-identity":
                    return '  1) ' + 'AB' * 20 + ' "Anvil Development"\n'
                if args[1:] == ["list-keychains", "-d", "user"]:
                    return '    "/tmp/original.keychain-db"\n'
                return ""
            with patch.dict(os.environ, env, clear=True), patch.object(signing, "run", side_effect=run):
                signing.setup()
            directory = Path(Path(root + "/state").read_text())
            self.assertFalse((directory / "identity.p12").exists())
            exported = Path(root + "/env").read_text()
            self.assertIn('ANVIL_MACOS_SIGNING_IDENTITY=' + 'AB' * 20, exported)
            self.assertIn('signingIdentity', exported)
            self.assertNotIn('fixture', exported)
            self.assertEqual((directory / "search-list.json").read_text(), '["/tmp/original.keychain-db"]')

    def test_failed_import_removes_p12_and_keeps_cleanup_state(self):
        with tempfile.TemporaryDirectory() as root:
            env = {"RUNNER_TEMP": root, "GITHUB_STATE_FILE": root + "/state",
                   "ANVIL_MACOS_CERTIFICATE_P12": "Zml4dHVyZQ=="}
            with patch.dict(os.environ, env, clear=True), patch.object(signing, "run", side_effect=RuntimeError("fixture")):
                with self.assertRaises(RuntimeError):
                    signing.setup()
            directory = Path(Path(root + "/state").read_text())
            self.assertFalse((directory / "identity.p12").exists())

    def test_cleanup_restores_search_list_before_deleting_keychain(self):
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root) / "signing"
            directory.mkdir()
            (directory / "search-list.json").write_text('["/tmp/original.keychain-db"]')
            (directory / "signing.keychain-db").touch()
            with patch.object(signing, "run") as run:
                signing.cleanup(directory)
            self.assertEqual(run.call_args_list[0].args[0],
                             ["security", "list-keychains", "-d", "user", "-s", "/tmp/original.keychain-db"])
            self.assertEqual(run.call_args_list[1].args[0][1], "delete-keychain")
            self.assertFalse(directory.exists())


if __name__ == '__main__':
    unittest.main()
