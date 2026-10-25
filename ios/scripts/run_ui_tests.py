#!/usr/bin/env python3
"""Run the same UI suite on an available iPhone and iPad simulator."""
import argparse, datetime, json, os, pathlib, subprocess, sys
root = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--output', type=pathlib.Path, default=pathlib.Path('/tmp') / ('mealos-ui-' + datetime.datetime.now().strftime('%Y%m%d-%H%M%S')))
parser.add_argument('--iphone', default=os.environ.get('TEST_IPHONE_UDID'), help='Optional simulator UDID')
parser.add_argument('--ipad', default=os.environ.get('TEST_IPAD_UDID'), help='Optional simulator UDID')
args = parser.parse_args()
env = os.environ.copy()
if not env.get('DEVELOPER_DIR') and pathlib.Path('/Applications/Xcode.app/Contents/Developer').exists():
    env['DEVELOPER_DIR'] = '/Applications/Xcode.app/Contents/Developer'
devices = json.loads(subprocess.check_output(['xcrun', 'simctl', 'list', 'devices', 'available', '--json'], env=env))['devices']
available = [d for runtime in reversed(sorted(devices)) for d in devices[runtime] if d.get('isAvailable')]
def choose(prefix, requested):
    for d in available:
        matches = d['udid'] == requested if requested else d['name'].startswith(prefix)
        if matches: return d
    raise SystemExit(f'No available {prefix} simulator. Install an iOS runtime in Xcode Settings > Components.')
args.output.mkdir(parents=True, exist_ok=True)
failed = False
for kind, device in [('iphone', choose('iPhone', args.iphone)), ('ipad', choose('iPad', args.ipad))]:
    result = args.output / (kind + '.xcresult')
    log = args.output / (kind + '.log')
    command = ['xcodebuild', '-project', str(root/'Forkware.xcodeproj'), '-scheme', 'Forkware', '-configuration', 'Debug', '-destination', 'platform=iOS Simulator,id=' + device['udid'], '-derivedDataPath', str(args.output / 'DerivedData'), '-resultBundlePath', str(result), '-parallel-testing-enabled', 'NO', 'CODE_SIGNING_ALLOWED=NO', 'test']
    print(f'Testing {device["name"]}; log: {log}', flush=True)
    with log.open('w') as output:
        run = subprocess.run(command, env=env, stdout=output, stderr=subprocess.STDOUT)
    if result.exists():
        subprocess.run(['xcrun','xcresulttool','export','attachments','--path',str(result),'--output-path',str(args.output/(kind+'-attachments'))],env=env,check=False)
    if run.returncode:
        failed = True
        print('\n'.join(log.read_text().splitlines()[-30:]))
    else: print(f'PASS {device["name"]}', flush=True)
print(f'Results: {args.output}')
sys.exit(1 if failed else 0)
