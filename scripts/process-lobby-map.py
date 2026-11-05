"""Reproducible owner-authorized nearest-neighbour map preparation and review overlay.
Run with the APFS image-tools Python (Pillow 11.3.0); the source PNG is never rewritten.
"""
import hashlib
import json
import subprocess
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT=Path(__file__).resolve().parents[1]
ART=ROOT/'app/src/assets/yard'
OUT=Path('/Volumes/TokensAbuse/dist/map-review');OUT.mkdir(parents=True,exist_ok=True)
source=ART/'lobby-source.png';raw=source.read_bytes();image=Image.open(source).convert('RGB')
data=json.loads((ART/'lobby-map.json').read_text())
assert image.size==(1672,941)
# Four source pixels per logical pixel; crop one bottom row rather than introduce a fractional scale.
# Nearest sampling is deterministic, and median-cut with no dithering removes noisy near-colours.
native=image.crop((0,0,1672,940)).resize((418,235),Image.Resampling.NEAREST)
processed=native.quantize(colors=96,method=Image.Quantize.MEDIANCUT,dither=Image.Dither.NONE).convert('RGB')
processed.save(ART/'lobby-native.png',optimize=False)
mask=Image.new('L',processed.size,0);ImageDraw.Draw(mask).polygon([tuple(p) for p in data['occluder']['polygon']],fill=255)
monitor=processed.convert('RGBA');monitor.putalpha(mask);monitor.save(ART/'lobby-monitor.png',optimize=False)
source_view=native.resize((836,470),Image.Resampling.NEAREST)
processed_view=processed.resize((836,470),Image.Resampling.NEAREST)
source_view.save(OUT/'source-at-native.png');processed_view.save(OUT/'processed-at-native.png')
comparison=Image.new('RGB',(1672,494),'#132f35');comparison.paste(source_view,(0,24));comparison.paste(processed_view,(836,24))
font=ImageFont.truetype(str(ROOT/'app/src/assets/fonts/PixelifySans.ttf'),16)
draw=ImageDraw.Draw(comparison);draw.text((8,2),'SOURCE: nearest only',font=font,fill='white');draw.text((844,2),'SELECTED: native grid, 96 colours',font=font,fill='white');comparison.save(OUT/'source-vs-processed.png')
overlay=processed.resize((1672,940),Image.Resampling.NEAREST);draw=ImageDraw.Draw(overlay)
def point(p):return (p['x']*4,p['y']*4)
for obstacle in data['obstacles']:draw.polygon([(x*4,y*4) for x,y in obstacle['points']],outline='#ff514d',width=2)
draw.polygon([(x*4,y*4) for x,y in data['occluder']['polygon']],outline='#d46bff',width=3)
for edge in data['edges']:
    draw.line([point(data['nodes'][edge['from']]),point(data['nodes'][edge['to']])],fill='#d46bff' if edge.get('behindOccluder') else '#77f8ff',width=3)
for name,p in data['nodes'].items():
    x,y=point(p);draw.rectangle((x-2,y-2,x+2,y+2),fill='#eeffff')
for station in data['stations']:
    entry=data['nodes'][station['entry']]
    for slot in station['feet']:
        draw.line([point(entry),point(slot)],fill='#ffd04f',width=2)
        x,y=point(slot);draw.ellipse((x-6,y-6,x+6,y+6),fill='#ffe94c',outline='#10292c',width=2)
    x,y=point(station['labelAnchor']);draw.rectangle((x,y,x+100,y+20),fill='#10292c');draw.text((x+2,y),station['label'],font=font,fill='white')
overlay.save(OUT/'path-and-anchors.png')
# Quantify source block coherence without claiming the AI-assisted image has one exact block lattice.
pixels=image.load();energies={}
for step in [2,3,4,5,6]:
    values=[]
    for offset in range(step):
        total=count=0
        for x in range(max(1,offset),image.width,step):
            for y in range(0,image.height,8):
                total+=sum(abs(a-b) for a,b in zip(pixels[x,y],pixels[x-1,y]));count+=1
        values.append(round(total/count,3))
    energies[str(step)]=values
manifest={'sourceProvenance':'Provided by Dion Arifi, 2026-11-05; original AI-assisted artwork','sourceSha256':hashlib.sha256(raw).hexdigest(),'sourceSize':list(image.size),'nativeSize':[418,235],'crop':[0,0,1672,940],'resample':'nearest neighbour; 4 source pixels per logical pixel','palette':'96 colours, median cut, no dither','nativeSha256':hashlib.sha256((ART/'lobby-native.png').read_bytes()).hexdigest(),'sourceColours':len(image.getcolors(image.width*image.height)),'nativeColours':len(processed.getcolors(418*235)),'edgeEnergyByCandidateBlock':energies,'choice':'4-pixel native grid matches the dominant apparent art scale; original has mixed AI-assisted block sizes. Compare both equally scaled review PNGs.','stations':len(data['stations']),'review':'Path/entrance overlay requires visual review; not a pixel-mask collision proof.'}
(ART/'processing.json').write_text(json.dumps(manifest,indent=2)+'\n')
subprocess.run([str(ROOT/'node_modules/.bin/prettier'),'--write',str(ART/'processing.json')],check=True)
assert source.read_bytes()==raw
print(json.dumps(manifest,indent=2))
