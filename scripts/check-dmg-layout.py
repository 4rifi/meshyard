"""Read-only final DMG format, source-art/layout and complete signed payload proof."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import subprocess
parser=argparse.ArgumentParser()
parser.add_argument('--base',type=Path,required=True)
parser.add_argument('--out',type=Path,required=True)
args=parser.parse_args()
assert str(args.base).startswith('/Volumes/TokensAbuse/') and str(args.out).startswith('/Volumes/TokensAbuse/')
args.out.mkdir(parents=True,exist_ok=True)
bundle=Path(os.environ['CARGO_TARGET_DIR'])/'aarch64-apple-darwin/release/bundle'
dmg=bundle/'dmg/Meshyard_0.3.0_aarch64.dmg'
mount=args.base/'layout-mount';mount.mkdir(parents=True,exist_ok=True)
def run(label,command):
    result=subprocess.run(command,capture_output=True,timeout=60)
    (args.out/(label+'.log')).write_bytes(result.stdout+result.stderr)
    assert result.returncode==0,(label,result.returncode)
    return result.stdout
info=run('imageinfo',['hdiutil','imageinfo','-plist',str(dmg)])
assert plistlib.loads(info)['Format']=='UDZO'
run('layout-attach',['hdiutil','attach','-readonly','-nobrowse','-mountpoint',str(mount),str(dmg)])
def manifest(root):
    files={};symlinks={}
    for p in sorted(root.rglob('*')):
        relative=str(p.relative_to(root))
        if p.is_symlink():symlinks[relative]=os.readlink(p)
        elif p.is_file():files[relative]=hashlib.sha256(p.read_bytes()).hexdigest()
    return {'filesSha256':files,'symlinks':symlinks}
try:
    built=manifest(bundle/'macos/Meshyard.app')
    copied=manifest(args.base/'Applications/Meshyard.app')
    mounted=manifest(mount/'Meshyard.app')
    assert built==copied==mounted
    assert (mount/'Applications').is_symlink() and os.readlink(mount/'Applications')=='/Applications'
    assert (mount/'.DS_Store').stat().st_size>0
    art=Path('assets/dmg-background.png').read_bytes()
    background=next(p for p in (mount/'.background').iterdir() if p.is_file() and p.read_bytes()==art)
    result={'format':'UDZO','payloadEquality':'Complete recursive files and symlinks match built, mounted and copied signed app','payload':built,'applicationsSymlink':'/Applications','layoutStoreBytes':(mount/'.DS_Store').stat().st_size,'pixelBackground':str(background.relative_to(mount)),'backgroundSha256':hashlib.sha256(art).hexdigest(),'finderWindow':'Not visually validated; no native Finder screenshot'}
    (args.out/'dmg-layout-payload.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
finally:
    run('layout-detach',['hdiutil','detach',str(mount)])
