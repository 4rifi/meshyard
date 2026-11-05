"""Verify and copy the built DMG on APFS without launching any native app."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import subprocess

parser=argparse.ArgumentParser();parser.add_argument('--base',type=Path,default=Path('/Volumes/TokensAbuse/tmp/phase4-package-030'));parser.add_argument('--out',type=Path,default=Path('/Volumes/TokensAbuse/logs/phase4'));args=parser.parse_args()
root=args.base;out=args.out
assert str(root).startswith('/Volumes/TokensAbuse/') and str(out).startswith('/Volumes/TokensAbuse/')
out.mkdir(parents=True,exist_ok=True)
dmg=Path(os.environ['CARGO_TARGET_DIR'])/'aarch64-apple-darwin/release/bundle/dmg/Meshyard_0.3.0_aarch64.dmg'
mount=root/'mount';app=root/'Applications/Meshyard.app'
if app.exists():raise SystemExit('Refuse to replace an existing copied app; coordinate its owned smoke first')
mount.mkdir(parents=True,exist_ok=True);app.parent.mkdir(parents=True,exist_ok=True)
checks={}
def run(name,args,expected=0):
    result=subprocess.run(args,capture_output=True)
    (out/(name+'.log')).write_bytes(result.stdout+result.stderr)
    checks[name]={'exit':result.returncode,'expected':expected,'command':args}
    if result.returncode!=expected:raise RuntimeError(name+' failed; inspect APFS log')
    return result.stdout
attached=False
try:
    run('dmg-verify',['hdiutil','verify',str(dmg)])
    run('dmg-attach',['hdiutil','attach','-readonly','-nobrowse','-mountpoint',str(mount),str(dmg)]);attached=True
    run('copy-app',['ditto',str(mount/'Meshyard.app'),str(app)])
finally:
    if attached:run('dmg-detach',['hdiutil','detach',str(mount)])
run('codesign-verify',['codesign','--verify','--deep','--strict','--verbose=2',str(app)])
run('codesign-details',['codesign','--display','--verbose=4',str(app)])
run('spctl',['spctl','--assess','--type','execute','--verbose=4',str(app)],3)
run('strict-bundle',['node','scripts/check-strict-bundle.mjs',str(app)])
main=app/'Contents/MacOS/meshyard-app';hook=app/'Contents/MacOS/meshyard-hook'
architecture=run('architecture',['file',str(main),str(hook)]).decode()
assert architecture.count('arm64')==2
info=plistlib.loads((app/'Contents/Info.plist').read_bytes())
assert info['CFBundleShortVersionString']=='0.3.0' and info['CFBundleIdentifier']=='ch.arifi.meshyard'
assert any('meshyard' in x['CFBundleURLSchemes'] for x in info['CFBundleURLTypes'])
extract=root/'extracted';extract.mkdir(exist_ok=True)
import shutil
shutil.copy2(hook,extract/'meshyard-hook')
assert (extract/'meshyard-hook').read_bytes()==hook.read_bytes()
result={'version':'0.3.0','dmg':str(dmg),'copiedApp':str(app),'sha256':hashlib.sha256(dmg.read_bytes()).hexdigest(),'hookSha256':hashlib.sha256(hook.read_bytes()).hexdigest(),'mainSha256':hashlib.sha256(main.read_bytes()).hexdigest(),'checks':checks,'nativeSmoke':'Prepared for parent; not executed by this script','keyStoreBoundary':'Explicit test file profiles only; production Keychain first launch remains not validated'}
(out/'package-results.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='checks'},indent=2))
