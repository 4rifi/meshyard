#!/usr/bin/env bash
# Resume only a Tauri-produced Meshyard image after a normal detach failure.
set -euo pipefail
source /Volumes/TokensAbuse/env.sh
python3 - "$@" <<'PY'
import hashlib,json,os,subprocess,sys,time
from pathlib import Path
if len(sys.argv)!=2:raise SystemExit('Usage: finish-dmg-after-ebusy.sh <exact Tauri rw image>')
base=Path(os.environ['CARGO_TARGET_DIR'])/'aarch64-apple-darwin/release/bundle'
image=Path(sys.argv[1]).resolve();output=base/'dmg/Meshyard_0.3.0_aarch64.dmg'
if image.parent!=base/'macos' or not image.name.startswith('rw.') or not image.name.endswith('.Meshyard_0.3.0_aarch64.dmg'):raise SystemExit('Refuse image outside this build')
if not image.is_file() or output.exists():raise SystemExit('Require existing rw image and absent output')
log=Path('/Volumes/TokensAbuse/logs')/('dmg-recovery-'+image.name);log.mkdir(exist_ok=False)
def run(label,args):
 r=subprocess.run(args,capture_output=True,timeout=60);(log/(label+'.log')).write_bytes(r.stdout+r.stderr)
 if r.returncode:raise SystemExit(f'{label} exit {r.returncode}; retain owned mount and inspect {log}')
 return r.stdout
import plistlib
info=plistlib.loads(run('mounted-images',['hdiutil','info','-plist']))
for item in info['images']:
 if Path(item.get('image-path','')).resolve()==image:
  entities=item.get('system-entities',[])
  device=next((e['dev-entry'] for e in entities if e.get('content-hint')=='GUID_partition_scheme'),None)
  if not device:raise SystemExit('Cannot identify exact owned device')
  run('normal-detach',['hdiutil','detach',device])
run('convert',['hdiutil','convert',str(image),'-format','UDZO','-o',str(output)])
run('verify',['hdiutil','verify',str(output)])
run('imageinfo',['hdiutil','imageinfo',str(output)])
(log/'result.json').write_text(json.dumps({'image':str(image),'output':str(output),'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),'note':'Compression resumed after Tauri EBUSY; no force, generated-script edits or payload changes. Full package/layout/native acceptance still required.'},indent=2)+'\n')
print(log)
PY
