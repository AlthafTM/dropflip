"""Draw Dropflip's circular arrows on a reflected black-glass tile.

Frame treatment follows the adjacent MIT-licensed Dictately project.
The conversion glyph is new vector geometry, not an edited bitmap.
"""
from pathlib import Path
import math
from PIL import Image, ImageChops, ImageDraw, ImageFilter

root = Path(__file__).resolve().parent
size = 1024
outer = Image.new('L', (size, size))
ImageDraw.Draw(outer).rounded_rectangle((20, 20, 1003, 1003), radius=215, fill=255)
inner = Image.new('L', (size, size))
ImageDraw.Draw(inner).rounded_rectangle((34, 34, 989, 989), radius=201, fill=255)
rim = ImageChops.subtract(outer, inner)
reflection = Image.new('RGBA', (size, size))
pixels = reflection.load()
for y in range(size):
    for x in range(size):
        key = math.exp(-(((x-180)/640)**2+((y-110)/620)**2))
        bounce = math.exp(-(((x-920)/420)**2+((y-880)/520)**2))
        shade = round(min(255, 83+169*key+121*bounce))
        pixels[x, y] = (shade, shade, shade, 255)
image = Image.new('RGBA', (size, size))
image.paste((6, 6, 7, 255), (0, 0, size, size), outer)
wide = rim.filter(ImageFilter.GaussianBlur(58)).point(lambda a: min(255, round(a*2.3)))
near = rim.filter(ImageFilter.GaussianBlur(15)).point(lambda a: min(255, round(a*1.15)))
image.paste(reflection, (0, 0), ImageChops.multiply(ImageChops.add(wide, near), outer))
image.paste(reflection, (0, 0), rim)

glyph = Image.new('L', (size, size))
draw = ImageDraw.Draw(glyph)
radius, thickness = 284, 86
point = lambda angle: (512+radius*math.cos(math.radians(angle)), 512+radius*math.sin(math.radians(angle)))
def stroke(points, width):
    draw.line(points, fill=255, width=width, joint='curve')
    for x, y in (points[0], points[-1]):
        draw.ellipse((x-width/2, y-width/2, x+width/2, y+width/2), fill=255)
for start in (210, 30):
    end = start+138
    edge = radius+thickness/2
    draw.arc((512-edge,512-edge,512+edge,512+edge),start,end,fill=255,width=thickness)
    for x,y in (point(start),point(end)):
        draw.ellipse((x-thickness/2,y-thickness/2,x+thickness/2,y+thickness/2),fill=255)
    x, y = point(end)
    angle = math.radians(end)
    tangent = (-math.sin(angle), math.cos(angle))
    radial = (math.cos(angle), math.sin(angle))
    tip = (x+12*tangent[0], y+12*tangent[1])
    shoulders = [(tip[0]-90*tangent[0]+side*64*radial[0], tip[1]-90*tangent[1]+side*64*radial[1]) for side in (-1, 1)]
    stroke([shoulders[0], tip, shoulders[1]], thickness)

halo = glyph.filter(ImageFilter.GaussianBlur(20)).point(lambda a: round(a*.24))
image.paste((220, 222, 228, 255), (0, 0, size, size), ImageChops.multiply(halo, outer))
ink = Image.new('RGBA', (size, size))
paint = ImageDraw.Draw(ink)
for y in range(size):
    t = y/(size-1)
    value = round(255-53*(t*t*(3-2*t)))
    paint.line((0, y, size, y), fill=(value, value, min(255, value+2), 255))
image.paste(ink, (0, 0), glyph)
image.putalpha(outer)
image.resize((512, 512), Image.Resampling.LANCZOS).save(root/'dropflip.png')
image.resize((192, 192), Image.Resampling.LANCZOS).save(root/'dropflip-preview.png')
image.save(root/'dropflip.ico', sizes=[(16,16),(20,20),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])

tray_mask = Image.new('L', (size, size))
symbol = glyph.crop(glyph.getbbox())
symbol.thumbnail((900, 900), Image.Resampling.LANCZOS)
tray_mask.paste(symbol, ((size-symbol.width)//2, (size-symbol.height)//2))
tray = Image.new('RGBA', (size, size), (240, 240, 242, 255))
tray.putalpha(tray_mask)
tray.resize((32, 32), Image.Resampling.LANCZOS).save(root/'dropflip-tray.png')
