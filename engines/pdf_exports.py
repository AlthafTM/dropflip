"""Page-complete exports and image routes using the existing packaged runtime."""
import json
import math
from io import BytesIO
from pathlib import Path
import sys
import zipfile

MAX_RASTER_PIXELS = 32_000_000
MAX_RASTER_EDGE = 16_384


def check_raster_size(page, dpi=72):
    width, height = page.rect.width, page.rect.height
    if not all(math.isfinite(value) and value > 0 for value in (width, height, dpi)):
        raise ValueError('Invalid page dimensions.')
    width, height = math.ceil(width * dpi / 72), math.ceil(height * dpi / 72)
    if max(width, height) > MAX_RASTER_EDGE or width * height > MAX_RASTER_PIXELS:
        raise ValueError('Page is too large to render safely (32 million pixels maximum).')


def progress(index, total):
    print(f'dropform_progress={0.05 + .9 * (index + 1) / max(total, 1):.4f}', flush=True)


def convert(source, output, quality):
    target = Path(output)
    suffix = target.suffix.lower()
    name = target.name.lower()
    quality = max(1, min(100, quality))
    if Path(source).suffix.lower() != '.pdf':
        from PIL import Image, ImageOps
        if suffix == '.json':
            with Image.open(source) as image:
                count = getattr(image, 'n_frames', 1)
                duration = 0
                for index in range(count):
                    image.seek(index)
                    duration += image.info.get('duration', 100)
                target.write_text(json.dumps({'width': image.width, 'height': image.height, 'frames': count, 'duration': duration / 1000}), encoding='utf-8')
            return
        if Path(source).suffix.lower() == '.svg':
            import pymupdf
            with pymupdf.open(source) as vector:
                if suffix == '.pdf':
                    target.write_bytes(vector.convert_to_pdf())
                    return
                check_raster_size(vector[0])
                pixmap = vector[0].get_pixmap(alpha=True)
                image = Image.open(BytesIO(pixmap.tobytes('png')))
                write_image(image, target, quality)
            return
        with Image.open(source) as image:
            count = getattr(image, 'n_frames', 1)
            if suffix == '.apng':
                write_apng(image, target)
                return
            if suffix == '.pdf':
                import pymupdf
                with pymupdf.open() as document:
                    for index in range(count):
                        image.seek(index)
                        frame = ImageOps.exif_transpose(image).convert('RGBA')
                        data = BytesIO()
                        frame.save(data, format='PNG')
                        with pymupdf.open('png', data.getvalue()) as picture:
                            with pymupdf.open('pdf', picture.convert_to_pdf()) as page:
                                document.insert_pdf(page)
                        progress(index, count)
                    document.save(target, garbage=3, deflate=True)
                return
            if suffix == '.zip':
                ext = name.split('.')[-2]
                with zipfile.ZipFile(target, 'w', compression=zipfile.ZIP_STORED) as archive:
                    for index in range(count):
                        image.seek(index)
                        frame = ImageOps.exif_transpose(image)
                        data = BytesIO()
                        if ext == 'jpg':
                            white(frame).save(data, format='JPEG', quality=quality)
                        else:
                            frame.save(data, format='PNG')
                        archive.writestr(f'page-{index+1:04}.{ext}', data.getvalue())
                        progress(index, count)
                return
            if count > 1:
                raise ValueError('Choose a page archive or PDF to preserve every frame.')
            write_image(ImageOps.exif_transpose(image), target, quality)
        return

    import pymupdf
    with pymupdf.open(source) as document:
        if document.needs_pass:
            raise ValueError('Unlock this PDF before conversion.')
        if suffix == '.txt':
            root = Path(getattr(sys, '_MEIPASS', Path(__file__).parent)) / 'tessdata'
            nonempty = False
            with target.open('w', encoding='utf-8', newline='\n') as stream:
                for index, page in enumerate(document):
                    text = page.get_text('text', sort=True)
                    if not text.strip() and page.get_images():
                        check_raster_size(page, 200)
                        textpage = page.get_textpage_ocr(language='eng+ind', dpi=200, full=True, tessdata=str(root))
                        text = page.get_text('text', textpage=textpage, sort=True)
                    nonempty = nonempty or bool(text.strip())
                    stream.write(text)
                    stream.write('\n\f\n' if index + 1 < len(document) else '\n')
                    progress(index, len(document))
            if not nonempty:
                raise ValueError('No readable text was found, including OCR.')
            return
        if suffix == '.xlsx':
            import xlsxwriter
            found = 0
            with xlsxwriter.Workbook(target, {'constant_memory': True, 'strings_to_formulas': False, 'strings_to_urls': False}) as book:
                for index, page in enumerate(document):
                    tables = page.find_tables()
                    for number, table in enumerate(tables.tables):
                        found += 1
                        sheet = book.add_worksheet(f'Page {index+1} Table {number+1}')
                        for row, cells in enumerate(table.extract()):
                            for column, value in enumerate(cells):
                                if value is not None:
                                    sheet.write_string(row, column, str(value))
                        sheet.set_column(0, max(table.col_count-1, 0), 20)
                    progress(index, len(document))
            if not found:
                raise ValueError('No digital tables were detected. Scanned-table recognition is not supported; no spreadsheet was published.')
            return
        ext = name.split('.')[-2] if suffix == '.zip' else suffix[1:]
        if ext not in ('png', 'jpg', 'svg'):
            raise ValueError('Unsupported page export.')
        if suffix != '.zip' and len(document) != 1:
            raise ValueError('Choose a pages ZIP to preserve every page.')
        archive = zipfile.ZipFile(target, 'w', compression=zipfile.ZIP_STORED) if suffix == '.zip' else None
        try:
            for index, page in enumerate(document):
                if ext == 'svg':
                    data = page.get_svg_image(text_as_path=True).encode('utf-8')
                else:
                    check_raster_size(page, 180)
                    pix = page.get_pixmap(dpi=180, alpha=ext == 'png', colorspace=pymupdf.csRGB)
                    data = pix.tobytes('jpeg', jpg_quality=quality) if ext == 'jpg' else pix.tobytes('png')
                if archive:
                    archive.writestr(f'page-{index+1:04}.{ext}', data)
                else:
                    target.write_bytes(data)
                progress(index, len(document))
        finally:
            if archive:
                archive.close()


def white(image):
    from PIL import Image
    rgba = image.convert('RGBA')
    background = Image.new('RGB', rgba.size, 'white')
    background.paste(rgba, mask=rgba.getchannel('A'))
    return background


def write_apng(image, output):
    """Write composed RGBA frames sequentially, keeping only one frame in memory."""
    import struct
    import zlib
    from fractions import Fraction
    count = getattr(image, 'n_frames', 1)
    sequence = 0
    with output.open('wb') as stream:
        def chunk(kind, data):
            stream.write(struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data) & 0xffffffff))
        stream.write(b'\x89PNG\r\n\x1a\n')
        for index in range(count):
            image.seek(index)
            frame = image.convert('RGBA')
            encoded = BytesIO()
            frame.save(encoded, format='PNG', compress_level=1)
            raw = encoded.getvalue()
            if index == 0:
                chunk(b'IHDR', raw[16:29])
                chunk(b'acTL', struct.pack('>II', count, image.info.get('loop', 0)))
            delay = Fraction(max(1, int(image.info.get('duration', 100))), 1000).limit_denominator(65535)
            chunk(b'fcTL', struct.pack('>IIIIIHHBB', sequence, frame.width, frame.height, 0, 0, delay.numerator, delay.denominator, 0, 0))
            sequence += 1
            offset = 8
            while offset < len(raw):
                length = struct.unpack('>I', raw[offset:offset+4])[0]
                kind, data = raw[offset+4:offset+8], raw[offset+8:offset+8+length]
                if kind == b'IDAT':
                    if index == 0:
                        chunk(b'IDAT', data)
                    else:
                        chunk(b'fdAT', struct.pack('>I', sequence) + data)
                        sequence += 1
                offset += length + 12
            progress(index, count)
        chunk(b'IEND', b'')


def write_image(image, output, quality):
    suffix = output.suffix.lower()
    if suffix == '.jpg':
        white(image).save(output, format='JPEG', quality=quality)
    elif suffix == '.avif':
        image.save(output, format='AVIF', quality=quality, max_threads=2, icc_profile=image.info.get('icc_profile'))
    elif suffix == '.ico':
        image.thumbnail((256, 256))
        image.save(output, format='ICO')
    elif suffix == '.webp':
        image.save(output, format='WEBP', quality=quality, lossless=quality==100)
    else:
        image.save(output)
