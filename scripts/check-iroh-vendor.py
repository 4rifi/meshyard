"""Replay both documented patches against the hash-verified upstream crate on APFS."""
import hashlib
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile

archive=Path(sys.argv[1])
assert hashlib.sha256(archive.read_bytes()).hexdigest()=='885787b892b5e2507c701f132ecbd45d2bad4bbb75157a19427087c16dadb833'
root=Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='p4-vendor-',dir='/Volumes/TokensAbuse/tmp') as temporary:
    with tarfile.open(archive,'r:gz') as source:
        for member in source.getmembers():
            relative=Path(member.name)
            assert not relative.is_absolute() and '..' not in relative.parts
            assert relative.parts[0]=='iroh-1.3.0' and (member.isfile() or member.isdir())
            destination=Path(temporary)/relative
            if member.isdir():destination.mkdir(parents=True,exist_ok=True)
            else:
                destination.parent.mkdir(parents=True,exist_ok=True)
                destination.write_bytes(source.extractfile(member).read())
    pristine=Path(temporary)/'iroh-1.3.0'
    for patch in ['iroh-1.3.0.patch','iroh-1.3.0-strict.patch']:
        subprocess.run(['patch','-p1','--input',str(root/'vendor'/patch)],cwd=pristine,check=True)
    count=0
    for original in pristine.rglob('*'):
        if original.is_file():
            target=root/'vendor/iroh-1.3.0'/original.relative_to(pristine)
            assert target.read_bytes()==original.read_bytes(),str(original.relative_to(pristine))+' differs from replayed upstream'
            count+=1
    assert count>=84
    print('PASS:',count,'upstream files exactly reproduced by TLS + Strict patches; original archive SHA256 verified')
