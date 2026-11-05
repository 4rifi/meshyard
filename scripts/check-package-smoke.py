"""Parent-only native smoke: direct executable, explicit isolated file profile, owned PID only."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('--app', type=Path, required=True)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
if not str(args.app.resolve()).startswith('/Volumes/TokensAbuse/') or not str(args.out.resolve()).startswith('/Volumes/TokensAbuse/'):
    raise SystemExit('Smoke app and output must remain on the APFS image')
args.out.mkdir(parents=True, exist_ok=True)
profile = Path(tempfile.mkdtemp(prefix='p4-native-', dir='/Volumes/TokensAbuse/tmp'))
home = profile / 'home'
home.mkdir(mode=0o700)
env = dict(os.environ, HOME=str(home), MESHYARD_DATA_DIR=str(profile/'data'),
           MESHYARD_HOOK_HOME=str(home), MESHYARD_KEY_STORE='file', MESHYARD_TEST_DISABLE_MAPPING='1')
log = args.out / 'native-smoke.log'
result = {'evidence':'Native launch smoke only; explicit TEST file-key profile, no production-service Keychain validation',
          'app':str(args.app), 'profile':str(profile), 'seconds':12}
with log.open('wb') as output:
    child = subprocess.Popen([str(args.app/'Contents/MacOS/meshyard-app')], env=env, stdout=output, stderr=subprocess.STDOUT)
    result['ownedPid'] = child.pid
    try:
        time.sleep(12)
        result['staysUp'] = child.poll() is None
    finally:
        if child.poll() is None:
            child.terminate()
            child.wait(timeout=15)
        result['terminatedOnlyOwnedChild'] = True
result['logEmpty'] = log.stat().st_size == 0
(args.out/'native-smoke.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
if not result['staysUp'] or not result['logEmpty']:
    raise SystemExit(1)
