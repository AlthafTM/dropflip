"""Real conversion tests. Run with Python + Pillow and FFmpeg on PATH."""
import hashlib
import json
import subprocess
import sys
import time
import zipfile
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
EXE = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'target/release/dropflip.exe'
WORK = ROOT / 'qa/output' / str(int(time.time()))
WORK.mkdir(parents=True)
results = []

def run(*args):
    p = subprocess.run([str(a) for a in args], capture_output=True, text=True, encoding='utf-8', timeout=120)
    if p.returncode:
        raise AssertionError(p.stderr)
    return p.stdout

def convert(src, ext, compress=False, strength=40):
    start = time.perf_counter()
    value = json.loads(run(EXE, '--convert', src, ext, WORK / 'converted', *(['--compress','--compression',str(strength)] if compress else [])))
    path = Path(value['path'])
    assert path.is_file() and path.stat().st_size > 0
    results.append({'input': Path(src).name, 'format': ext, 'compress': compress, 'strength':strength, 'seconds': round(time.perf_counter()-start, 3), 'bytes': path.stat().st_size})
    return path

img = WORK / 'image.png'
Image.effect_noise((640, 360), 45).convert('RGB').save(img)
before = hashlib.sha256(img.read_bytes()).hexdigest()
for fmt in ['jpg', 'webp', 'bmp', 'tiff', 'ico', 'tga', 'qoi', 'png']:
    out = convert(img, fmt, True)
    with Image.open(out) as decoded:
        decoded.load()
        assert decoded.width == (256 if fmt == 'ico' else 640)
pdf = convert(img, 'pdf')
assert pdf.read_bytes().startswith(b'%PDF')
assert hashlib.sha256(img.read_bytes()).hexdigest() == before
low=convert(img,'jpg',True,10)
high=convert(img,'jpg',True,90)
assert high.stat().st_size < low.stat().st_size
small_pdf=convert(pdf,'pdf',True,90)
assert small_pdf.stat().st_size < pdf.stat().st_size
pdf_jpeg_high=convert(pdf,'jpg',True,10)
pdf_jpeg_low=convert(pdf,'jpg',True,90)
assert pdf_jpeg_low.stat().st_size < pdf_jpeg_high.stat().st_size
pdf_options=json.loads(run(EXE,'--inspect',pdf))['formats']
assert not next(f for f in pdf_options if f['ext']=='png')['compression']
raster_word=convert(pdf,'docx')
with zipfile.ZipFile(raster_word) as package:
    assert any(name.startswith('word/media/') for name in package.namelist())

md = WORK / 'note.md'
md.write_text('# Conversion test\n\nA real document with **bold text**.\n\n- First\n- Second\n', encoding='utf-8')
for fmt in ['docx', 'html', 'epub', 'odt', 'rtf', 'tex.zip', 'rst.zip', 'txt', 'pdf']:
    out = convert(md, fmt)
    if fmt == 'docx':
        assert 'Conversion test' in run(ROOT / 'tools/pandoc.exe', out, '-t', 'plain')
    if fmt == 'pdf':
        assert 'Conversion test' in convert(out, 'txt').read_text(encoding='utf-8')
        word=convert(out,'docx')
        assert 'Conversion test' in run(ROOT / 'tools/pandoc.exe',word,'-t','plain')
        convert(out, 'pdf', True)

layout=ROOT/'qa/layout'
if (layout/'source.pdf').exists():
    layout_word=convert(layout/'source.pdf','docx')
    with zipfile.ZipFile(layout_word) as package:
        body=package.read('word/document.xml').decode('utf-8')
        assert '<w:tbl>' in body and 'Alpha' in body
        assert any(name.startswith('word/media/') for name in package.namelist())
    layout_pdf=convert(layout_word,'pdf')
    assert json.loads(run(EXE,'--inspect',layout_pdf))['summary']=='2 pages'
    ocr_word=convert(layout/'scan.pdf','docx')
    assert 'Scanned document check' in run(ROOT/'tools/pandoc.exe',ocr_word,'-t','plain')
    slides=convert(layout/'source.pdf','pptx')
    with zipfile.ZipFile(slides) as package:
        slide_xml=[package.read(name).decode('utf-8') for name in package.namelist() if name.startswith('ppt/slides/slide') and name.endswith('.xml')]
        assert len(slide_xml)==2 and 'Conversion layout check' in slide_xml[0] and 'Column 2, editable line 10.' in slide_xml[1]
    slide_pdf=convert(slides,'pdf')
    assert json.loads(run(EXE,'--inspect',slide_pdf))['summary']=='2 pages'
    legacy=convert(layout/'source.pdf','ppt')
    assert legacy.read_bytes().startswith(bytes.fromhex('D0CF11E0A1B11AE1'))
    vector=convert(layout/'source.pdf','svg.zip')
    with zipfile.ZipFile(vector) as package:
        assert len(package.namelist())==2
        assert all(b'<svg' in package.read(name) for name in package.namelist())
    scan_slides=convert(layout/'scan.pdf','pptx')
    with zipfile.ZipFile(scan_slides) as package:
        assert any(name.startswith('ppt/media/') for name in package.namelist())

csv = WORK / 'records.csv'
csv.write_text('name,value\nalpha,0042\nbeta,17\n', encoding='utf-8')
data = convert(csv, 'json')
assert json.loads(data.read_text())[0]['value'] == '0042'
for fmt in ['yaml', 'toml', 'csv', 'tsv']:
    if fmt == 'toml':
        obj = WORK / 'config.json'
        obj.write_text('{"title":"hello","count":3}')
        convert(obj, fmt)
    else:
        convert(data, fmt)
archive = convert(md, 'zip', True)
with zipfile.ZipFile(archive) as z:
    assert z.read(md.name) == md.read_bytes()
assert convert(csv, 'json') != data

video = WORK / 'sample.mp4'
run('ffmpeg', '-v', 'error', '-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=24', '-f', 'lavfi', '-i', 'sine=frequency=440', '-t', '3', '-c:v', 'libx264', '-threads', '2', '-c:a', 'aac', video)
for fmt in ['mkv', 'mp4', 'webm', 'mov', 'gif', 'mp3', 'm4a', 'opus', 'ogg', 'flac', 'wav', 'aiff']:
    out = convert(video, fmt, fmt not in ['mkv', 'mov'])
    probe = json.loads(run('ffprobe', '-v', 'error', '-show_streams', '-of', 'json', out))
    assert probe['streams']
    if fmt == 'mkv':
        assert probe['streams'][0]['codec_name'] == 'h264'
audio = next((WORK / 'converted').glob('*.mp3'))
info = json.loads(run(EXE, '--inspect', audio))
assert 'mp4' not in [f['ext'] for f in info['formats']]
bad = WORK / 'bad.json'
bad.write_text('broken')
p = subprocess.run([str(EXE), '--convert', str(bad), 'yaml', str(WORK / 'invalid')], capture_output=True)
assert p.returncode != 0
assert not list((WORK / 'invalid').glob('*'))
(ROOT / 'qa/results.json').write_text(json.dumps({'directory': str(WORK), 'checks': results, 'count': len(results)}, indent=2))
print(f'{len(results)} conversions validated; record preservation, collision handling, invalid data and audio format filtering passed. Results: {ROOT / "qa/results.json"}')
