"""Content checks for the expanded formats and repaired conversion routes."""
import csv
import hashlib
from io import BytesIO
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import zipfile
from PIL import Image
import pymupdf
import xlsxwriter

ROOT = Path(__file__).resolve().parents[1]
EXE = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT/'target/debug/dropflip.exe'
SECTION = os.environ.get('DROPFLIP_QA_SECTION', 'all')
WORK = ROOT/'qa/extended-output'/f'{int(time.time())}-{SECTION}'
WORK.mkdir(parents=True)
REPORT = ROOT/'qa'/f'extended-{SECTION}-results.json'
records = []
FFMPEG, FFPROBE = ROOT/'tools/ffmpeg.exe', ROOT/'tools/ffprobe.exe'


def run(*args):
    result = subprocess.run([str(a) for a in args], capture_output=True, encoding='utf-8', timeout=180)
    assert result.returncode == 0, result.stderr
    return result.stdout


def save():
    REPORT.write_text(json.dumps({'work':str(WORK),'checks':records,'count':len(records)},indent=2),encoding='utf-8')


def convert(source, fmt, strength=None):
    start = time.perf_counter()
    extra = ['--compress','--compression',str(strength)] if strength else []
    result = json.loads(run(EXE,'--convert',source,fmt,WORK/'converted',*extra))
    output = Path(result['path'])
    assert output.stat().st_size > 0
    records.append({'input':Path(source).name,'format':fmt,'seconds':round(time.perf_counter()-start,3),'bytes':output.stat().st_size})
    save()
    print(f'{Path(source).name} -> {fmt}',flush=True)
    return output


def options(source):
    return json.loads(run(EXE,'--inspect',source))


def probe(source):
    return json.loads(run(FFPROBE,'-v','error','-show_streams','-show_format','-show_chapters','-of','json',source))


if SECTION in ('all','images'):
    picture=WORK/'alpha.png'
    rgba=Image.new('RGBA',(128,64),(220,40,20,0))
    rgba.paste((20,80,220,255),(64,0,128,64));rgba.save(picture)
    import array
    highbit=WORK/'sixteen-bit.png'
    pixels=array.array('H',[(i*997)%65536 for i in range(1024)])
    Image.frombytes('I;16',(32,32),pixels.tobytes()).save(highbit,compress_level=0)
    compressed=Image.open(convert(highbit,'png',90))
    assert list(compressed.getdata())==list(pixels)
    avif=convert(picture,'avif')
    decoded=Image.open(convert(avif,'png')).convert('RGBA')
    assert decoded.size==(128,64) and decoded.getpixel((0,0))[3]==0 and decoded.getpixel((100,0))[3]==255
    convert(picture,'gif')
    jpeg=WORK/'rotated.jpg'
    exif=Image.Exif();exif[274]=6
    Image.new('RGB',(100,60),'red').save(jpeg,exif=exif)
    assert Image.open(convert(jpeg,'png')).size==(60,100)
    svg=WORK/'vector.svg';svg.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="200" height="100" fill="red"/><circle cx="100" cy="50" r="20" fill="blue"/></svg>')
    raster=Image.open(convert(svg,'png')).convert('RGB')
    assert raster.size==(200,100) and raster.getpixel((100,50))==(0,0,255)
    with pymupdf.open(convert(svg,'pdf')) as pdf:
        assert len(pdf)==1 and pdf[0].get_drawings() and not pdf[0].get_images()
    tiff=WORK/'scans.tiff'
    frames=[Image.new('RGB',(80,60),color) for color in ['red','green','blue']]
    frames[0].save(tiff,save_all=True,append_images=frames[1:])
    assert options(tiff)['frames']==3
    with zipfile.ZipFile(convert(tiff,'png.zip')) as pages:
        assert len(pages.namelist())==3
        assert Image.open(BytesIO(pages.read('page-0003.png'))).getpixel((0,0))==(0,0,255)
    with pymupdf.open(convert(tiff,'pdf')) as pdf:
        assert len(pdf)==3
    for fmt in ['png.zip','jpg.zip','svg.zip']:
        with zipfile.ZipFile(convert(ROOT/'qa/layout/source.pdf',fmt)) as pages:
            assert len(pages.namelist())==2
    source=convert(picture,'jpg')
    original=hashlib.sha256(source.read_bytes()).hexdigest()
    copy=convert(source,'jpg')
    assert hashlib.sha256(copy.read_bytes()).hexdigest()==original
    animated=WORK/'animated.webp'
    frames[0].save(animated,save_all=True,append_images=frames[1:],duration=[200,300,400],loop=0,lossless=True)
    assert options(animated)['kind']=='Video' and options(animated)['frames']==3
    proxy=WORK/'frames.apng'
    helper=Path(os.environ.get('DROPFORM_TOOLS', ROOT/'tools'))/'pdf-word/pdf-word.exe'
    run(helper,animated,proxy)
    with Image.open(proxy) as decoded:
        assert decoded.n_frames==3
        for index,(color,delay) in enumerate([((255,0,0),200),((0,128,0),300),((0,0,255),400)]):
            decoded.seek(index)
            assert decoded.convert('RGB').getpixel((0,0))==color and decoded.info['duration']==delay
    video=probe(convert(animated,'mp4'))
    assert float(video['format']['duration'])>=.8
    heic=ROOT/'qa/fixtures/libheif-example.heic'
    if heic.exists():
        image=Image.open(convert(heic,'jpg'));image.load();assert image.width>100 and image.height>100

if SECTION in ('all','sheets'):
    source=WORK/'identifiers.csv'
    source.write_text('id,formula,name\n0042,=1+1,Alpha\n0007,+44,Beta\n',encoding='utf-8')
    book=convert(source,'xlsx')
    value=json.loads(convert(book,'json').read_text(encoding='utf-8'))
    assert value['Data'][1]==['0042','=1+1','Alpha']
    back=convert(book,'csv')
    assert list(csv.reader(back.open()))==list(csv.reader(source.open()))
    for fmt in ['ods','pdf']:
        converted=convert(source,fmt)
        if fmt=='pdf':
            with pymupdf.open(converted) as doc:assert '0042' in ''.join(p.get_text() for p in doc)
    records_json=WORK/'typed.json'
    records_json.write_text('[{"id":123456789012345678,"value":42,"enabled":true,"code":"0042"}]')
    excel=convert(records_json,'xlsx')
    parsed=json.loads(convert(excel,'json').read_text())['Data']
    row=dict(zip(parsed[0],parsed[1]))
    assert row=={'id':'123456789012345678','value':42,'enabled':True,'code':'0042'}
    workbook=WORK/'two-sheets.xlsx'
    with xlsxwriter.Workbook(workbook) as x:
        x.add_worksheet('First').write_string(0,0,'0042')
        x.add_worksheet('Second').write_formula(0,0,'=40+2',None,42)
    assert 'csv' not in [f['ext'] for f in options(workbook)['formats']]
    for fmt in ['csv.zip','tsv.zip']:
        with zipfile.ZipFile(convert(workbook,fmt)) as sheets:
            assert len(sheets.namelist())==2 and sheets.read('02-Second.'+fmt.split('.')[0]).strip()==b'42'
    for fmt in ['ods','xls']:
        changed=convert(workbook,fmt)
        restored=convert(changed,'xlsx')
        parsed=json.loads(convert(restored,'json').read_text())
        assert parsed['First'][0][0]=='0042' and parsed['Second'][0][0]==42

if SECTION in ('all','documents'):
    plain=convert(ROOT/'qa/layout/scan.pdf','txt').read_text(encoding='utf-8')
    assert 'Scanned document check' in plain
    tables=convert(ROOT/'qa/layout/source.pdf','xlsx')
    parsed=json.loads(convert(tables,'json').read_text())
    assert any('Alpha' in str(rows) for rows in parsed.values())
    no_tables=WORK/'no-tables.pdf'
    with pymupdf.open() as doc:
        page=doc.new_page();page.insert_text((72,72),'A plain paragraph without table borders.');doc.save(no_tables)
    failed=subprocess.run([str(EXE),'--convert',str(no_tables),'xlsx',str(WORK/'failure')],capture_output=True)
    assert failed.returncode and not list((WORK/'failure').glob('*'))
    word=ROOT/'qa/layout/bundled.docx'
    if not word.exists():
        word=convert(ROOT/'qa/layout/source.pdf','docx')
    html=convert(word,'html').read_text(encoding='utf-8')
    assert 'data:image/' in html and 'src="media/' not in html
    for fmt in ['md.zip','tex.zip','rst.zip']:
        with zipfile.ZipFile(convert(word,fmt)) as bundle:
            assert f'document.{fmt.split(".")[0]}' in bundle.namelist()
            assert any(name.startswith('media/') for name in bundle.namelist())
    legacy=convert(word,'doc')
    assert legacy.read_bytes().startswith(bytes.fromhex('D0CF11E0A1B11AE1'))
    restored=convert(legacy,'docx')
    with pymupdf.open(convert(restored,'pdf')) as doc:
        assert len(doc)==2 and 'Conversion layout check' in doc[0].get_text()

if SECTION in ('all','media'):
    source=WORK/'tracks.mkv';subtitles=WORK/'captions.srt'
    subtitles.write_text('1\n00:00:00,100 --> 00:00:01,500\nConversion caption\n')
    attachment=WORK/'attachment.txt';attachment.write_text('Keep this attachment')
    run(FFMPEG,'-v','error','-f','lavfi','-i','testsrc2=size=320x180:rate=24','-f','lavfi','-i','sine=frequency=440','-f','lavfi','-i','sine=frequency=880','-i',subtitles,'-map','0:v','-map','1:a','-map','2:a','-map','3:s','-t','2','-c:v','libx264','-threads','2','-c:a','aac','-c:s','srt','-attach',attachment,'-metadata:s:t','mimetype=text/plain',source)
    for fmt in ['mp4','mov','webm','mkv']:
        streams=probe(convert(source,fmt,40))['streams']
        assert sum(s['codec_type']=='audio' for s in streams)==2
        assert sum(s['codec_type']=='subtitle' for s in streams)==1
        if fmt=='mkv':assert sum(s['codec_type']=='attachment' for s in streams)==1
    for fmt in ['aac','wma']:
        streams=probe(convert(source,fmt))['streams'];assert streams[0]['codec_type']=='audio'
    simple=WORK/'simple.mp4'
    run(FFMPEG,'-v','error','-i',source,'-map','0:v','-map','0:a:0','-c','copy',simple)
    assert probe(convert(simple,'avi'))['streams'][0]['codec_name']=='mpeg4'
    highres=WORK/'highres.wav'
    run(FFMPEG,'-v','error','-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-t','1','-c:a','pcm_s24le',highres)
    aiff=convert(highres,'aiff');assert probe(aiff)['streams'][0]['bits_per_sample']==24
    flac=convert(highres,'flac');assert probe(flac)['streams'][0]['bits_per_raw_sample']=='24'
    restored=convert(flac,'wav');assert probe(restored)['streams'][0]['codec_name']=='pcm_s24le'

save()
print(f'Validated {len(records)} outputs: {REPORT}',flush=True)
