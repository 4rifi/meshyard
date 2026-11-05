"""Regenerate the normalized vendored manifest from verified upstream and maintained source."""
import argparse
import hashlib
import io
import os
from pathlib import Path
import tarfile

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / 'vendor/iroh-1.3.0'
SHA256 = '885787b892b5e2507c701f132ecbd45d2bad4bbb75157a19427087c16dadb833'
parser = argparse.ArgumentParser()
parser.add_argument('--archive', type=Path)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
archives = list((Path(os.environ['CARGO_HOME']) / 'registry/cache').glob('*/iroh-1.3.0.crate'))
archive = args.archive or (archives[0] if len(archives) == 1 else None)
if archive is None:
    raise SystemExit('Supply the original iroh 1.3.0 crate archive with --archive')
raw = archive.read_bytes()
if hashlib.sha256(raw).hexdigest() != SHA256:
    raise SystemExit('Upstream archive checksum mismatch')
with tarfile.open(fileobj=io.BytesIO(raw), mode='r:gz') as package:
    def read(name):
        file = package.extractfile('iroh-1.3.0/' + name)
        if file is None:
            raise SystemExit('Missing upstream manifest')
        return file.read().decode()
    original = read('Cargo.toml.orig')
    normalized = read('Cargo.toml')
source = original.replace('[features]\n', '[features]\n# Meshyard release: compile out public relay and discovery defaults.\nstrict-only = []\n', 1)
if (VENDOR / 'Cargo.toml.orig').read_text() != source:
    raise SystemExit('Maintained source manifest differs from the approved feature-only delta')
output = normalized.replace('[features]\n', '[features]\n# Meshyard release: compile out public relay and discovery defaults.\nstrict-only = []\n', 1)
if args.check:
    if (VENDOR / 'Cargo.toml').read_text() != output:
        raise SystemExit('Generated normalized manifest is stale')
else:
    (VENDOR / 'Cargo.toml').write_text(output)
print('PASS: hash-verified upstream manifest plus source strict-only feature reproduces normalized Cargo.toml')
