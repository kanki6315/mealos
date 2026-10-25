"""Offline checks: never build, install, sign or upload an app."""
import contextlib
import io
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / '.flightdeck/release.sh'
RELEASE = next(ROOT / p for p in ('release_ios', 'release.sh', 'ios/release.sh') if (ROOT / p).is_file())
PHONE = '12345678-1234-1234-1234-123456789abc'
TABLET = '87654321-4321-4321-4321-cba987654321'


class ReleaseContractTests(unittest.TestCase):
    def shell(self, script, env=None, args=()):
        clean = {k: v for k, v in os.environ.items() if not k.startswith(('FLIGHTDECK_', 'TEST_'))}
        clean.update(env or {})
        return subprocess.run(['bash', '-c', 'set -euo pipefail; ' + script, 'test', str(HELPER), *args], env=clean, cwd=ROOT, capture_output=True, text=True, timeout=10)

    def managed(self, **extra):
        return dict(FLIGHTDECK_RUN_ID='test-only', TEST_DESTINATION='platform=iOS Simulator,id=' + PHONE, **extra)

    def test_shell_syntax_and_help(self):
        subprocess.run(['bash', '-n', str(RELEASE)], check=True)
        subprocess.run(['bash', '-n', str(HELPER)], check=True)
        result = subprocess.run(['bash', str(RELEASE), '--help'], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('--no-upload', result.stdout)

    def test_unmanaged_release_does_not_need_a_destination(self):
        result = self.shell('source "$1"; flightdeck_event "No machine output"; printf "%s" "${TEST_PARALLEL_ENABLED:-unset}"')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, 'unset')

    def test_events_preserve_special_characters_and_archive_path(self):
        with tempfile.TemporaryDirectory() as temp:
            event = Path(temp) / 'events.jsonl'
            result = self.shell('source "$1"; build=298.12.34; version=1.2; archive="build/My App.xcarchive"; flightdeck_event "$2"', {'FLIGHTDECK_EVENTS_FILE': str(event)}, ('A "quoted" stage\nwith a newline',))
            self.assertEqual(result.returncode, 0, result.stderr)
            lines = event.read_text().splitlines()
            self.assertEqual(len(lines), 1)
            payload = json.loads(lines[0])
            self.assertEqual(payload['stage'], 'A "quoted" stage\nwith a newline')
            self.assertEqual(payload['archive'], str(ROOT / 'build/My App.xcarchive'))
            self.assertEqual(payload['build'], '298.12.34')

    def test_managed_destination_is_required_before_preflight(self):
        result = self.shell('bash "' + str(RELEASE) + '" --no-upload', {'FLIGHTDECK_RUN_ID': 'test-only'})
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn('FlightDeck requires', result.stderr)

    def test_names_and_nonhex_uuids_are_rejected(self):
        for value in ('platform=iOS Simulator,name=iPhone', 'platform=iOS Simulator,id=zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz', 'platform=iOS Simulator,id=' + PHONE + ',OS=latest'):
            with self.subTest(destination=value):
                result = self.shell('source "$1"', dict(FLIGHTDECK_RUN_ID='test-only', TEST_DESTINATION=value))
                self.assertNotEqual(result.returncode, 0)

    def test_managed_runs_disable_clones_and_parallel_workers(self):
        result = self.shell('source "$1"; printf "%s/%s" "$TEST_PARALLEL_ENABLED" "$TEST_WORKERS"', self.managed(TEST_PARALLEL_ENABLED='YES', TEST_WORKERS='4'))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, 'NO/1')
        if not (ROOT / 'ios/scripts/run_ui_tests.py').exists():
            self.assertIn('${flightdeck_test_args[@]+"${flightdeck_test_args[@]}"}', RELEASE.read_text())

    def test_skipped_tests_do_not_require_devices(self):
        result = self.shell('run_tests=0; source "$1"', {'FLIGHTDECK_RUN_ID': 'test-only', 'FLIGHTDECK_REQUIRES_IPAD': '1'})
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_dual_device_requires_both_distinct_matching_ids(self):
        base = self.managed(FLIGHTDECK_REQUIRES_IPAD='1', TEST_IPHONE_UDID=PHONE, TEST_IPAD_UDID=TABLET)
        self.assertEqual(self.shell('source "$1"', base).returncode, 0)
        for changed in ({'TEST_IPAD_UDID': ''}, {'TEST_IPAD_UDID': PHONE}, {'TEST_IPHONE_UDID': TABLET}):
            with self.subTest(changed=changed):
                self.assertNotEqual(self.shell('source "$1"', dict(base, **changed)).returncode, 0)

    @unittest.skipUnless((ROOT / 'ios/scripts/run_ui_tests.py').exists(), 'Only Forkware has this test runner')
    def test_forkware_uses_env_devices_instead_of_first_available(self):
        inventory = {'devices': {'iOS': [
            {'udid': 'unleased-phone', 'name': 'iPhone first', 'isAvailable': True},
            {'udid': 'unleased-tablet', 'name': 'iPad first', 'isAvailable': True},
            {'udid': PHONE, 'name': 'iPhone leased', 'isAvailable': True},
            {'udid': TABLET, 'name': 'iPad leased', 'isAvailable': True},
        ]}}
        calls = []
        def fake_run(command, **kwargs):
            calls.append(command)
            return subprocess.CompletedProcess(command, 0)
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, {'TEST_IPHONE_UDID': PHONE, 'TEST_IPAD_UDID': TABLET}), patch.object(sys, 'argv', ['run_ui_tests.py', '--output', temp]), patch.object(subprocess, 'check_output', return_value=json.dumps(inventory).encode()), patch.object(subprocess, 'run', side_effect=fake_run), contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(SystemExit) as exit:
                runpy.run_path(str(ROOT / 'ios/scripts/run_ui_tests.py'), run_name='__main__')
            self.assertEqual(exit.exception.code, 0)
        destinations = [c[c.index('-destination') + 1] for c in calls if c[0] == 'xcodebuild']
        self.assertEqual(destinations, ['platform=iOS Simulator,id=' + PHONE, 'platform=iOS Simulator,id=' + TABLET])


if __name__ == '__main__':
    unittest.main()
