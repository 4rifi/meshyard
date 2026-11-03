"""Record one real check, preserving logs and source hashes; no background service."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--output-dir', type=Path, required=True)
parser.add_argument('command', nargs=argparse.REMAINDER)
args = parser.parse_args()
base = Path('/Volumes/MeshyardProjects/Meshyard-build/orchestration/t007b-connect-evidence')
destination = args.output_dir.resolve()
if not destination.is_relative_to(base) or destination == base:
    parser.error('use a fresh child of the USB connect evidence root')
if not args.command:
    parser.error('command required')
destination.mkdir(parents=True, exist_ok=False)
repo = Path(__file__).resolve().parent.parent
os.chdir(repo)
expected = '/Volumes/MeshyardProjects/Meshyard-build/targets/t-007b-local-agent-integration'
assert os.environ.get('CARGO_TARGET_DIR') == expected
assert os.environ.get('TMPDIR', '').startswith('/Volumes/MeshyardProjects/')

def inputs():
    names = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z']).split(b'\0')
    return {os.fsdecode(name): hashlib.sha256(Path(os.fsdecode(name)).read_bytes()).hexdigest() for name in names if name and Path(os.fsdecode(name)).is_file()}

def head():
    return subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()

source_head, before = head(), inputs()
(destination / 'inputs.json').write_text(json.dumps(before, indent=2) + '\n')
started = time.monotonic()
with (destination / 'output.log').open('wb') as output:
    completed = subprocess.run(args.command, stdout=output, stderr=subprocess.STDOUT)
elapsed = time.monotonic() - started
after, end_head = inputs(), head()
changed = sorted(key for key in before.keys() | after.keys() if before.get(key) != after.get(key))
raw = (destination / 'output.log').read_text(errors='replace')
result = {
    'command': args.command, 'commandExitCode': completed.returncode,
    'exitCode': completed.returncode or (125 if changed or source_head != end_head else 0),
    'wallSeconds': round(elapsed, 3), 'sourceHead': source_head, 'endHead': end_head,
    'sourceChangedDuringRun': changed, 'target': expected,
    'context': os.environ.get('MESHYARD_CHECK_CONTEXT', 'restricted worker; no normal-shell UDS/native proof'),
    'summaries': re.findall(r'(?:test result:.*|Tests\s+[^\n]*|Test Files\s+[^\n]*)', re.sub(r'\x1b\[[0-9;]*m', '', raw)),
}
(destination / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
raise SystemExit(result['exitCode'])
