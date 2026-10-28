"""Make reproducible born-digital and scanned PDFs for conversion fidelity checks."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
from reportlab.pdfgen import canvas
from reportlab.lib.utils import ImageReader

root = Path(__file__).resolve().parent / 'layout'
root.mkdir(exist_ok=True)
picture = Image.new('RGB', (420, 180), '#e7edf3')
d = ImageDraw.Draw(picture)
for index, value in enumerate([65, 112, 145, 95]):
    d.rectangle((30+index*95, 165-value, 85+index*95, 165), fill=['#426a8c','#5989aa','#6b9dba','#91b4c7'][index])
picture.save(root/'picture.png')
pdf = canvas.Canvas(str(root/'source.pdf'), pagesize=(595.28,841.89))
pdf.setTitle('Conversion layout check')
pdf.setFillColorRGB(.12,.22,.32); pdf.setFont('Helvetica-Bold',22)
pdf.drawString(50,780,'Conversion layout check')
pdf.setFillColorRGB(.15,.15,.15);pdf.setFont('Helvetica',11)
for row,line in enumerate(['This test document contains formatted paragraphs, a table and a raster image.',
                           'Its Word conversion should retain the page structure and editable content.']):
    pdf.drawString(50,748-row*17,line)
pdf.setFont('Helvetica-Bold',14);pdf.drawString(50,690,'Table structure')
xs=[50,280,380,545];ys=[666,634,602,570,538]
pdf.setFillColorRGB(.9,.93,.96);pdf.rect(50,634,495,32,fill=1,stroke=0)
pdf.setStrokeColorRGB(.45,.52,.58);pdf.setLineWidth(.6)
for x in xs:pdf.line(x,538,x,666)
for y in ys:pdf.line(50,y,545,y)
rows=[['Item','Quantity','Status'],['Alpha','12','Ready'],['Beta','7','Review'],['Gamma','24','Ready']]
for row,values in enumerate(rows):
    pdf.setFillColorRGB(.12,.15,.18);pdf.setFont('Helvetica-Bold' if row==0 else 'Helvetica',11)
    for col,value in enumerate(values):pdf.drawString(xs[col]+10,ys[row]-21,value)
pdf.setFont('Helvetica-Bold',14);pdf.drawString(50,494,'Embedded image')
pdf.drawImage(ImageReader(picture),50,280,width=420,height=180)
pdf.setFont('Helvetica',10);pdf.drawString(50,259,'Figure 1. A raster image used to verify picture preservation.')
pdf.setFont('Helvetica',9);pdf.drawRightString(545,40,'Page 1 of 2')
pdf.showPage()
pdf.setFont('Helvetica-Bold',20);pdf.drawString(50,780,'Second page')
pdf.setFont('Helvetica',11)
for col,x in enumerate([50,310]):
    for line in range(10):pdf.drawString(x,735-line*18,f'Column {col+1}, editable line {line+1}.')
pdf.setFont('Helvetica',9);pdf.drawRightString(545,40,'Page 2 of 2')
pdf.save()

scan = Image.new('RGB',(1240,1754),'white'); sd=ImageDraw.Draw(scan)
font=ImageFont.truetype('C:/Windows/Fonts/arial.ttf',28)
title=ImageFont.truetype('C:/Windows/Fonts/arialbd.ttf',44)
sd.text((95,110),'Scanned document check',font=title,fill='black')
for i,line in enumerate(['This page is an image with no embedded PDF text.',
                         'The conversion should recognize these words with OCR.',
                         'English and Indonesian language data are included.',
                         'Dokumen ini dapat diubah menjadi teks yang bisa diedit.']):
    sd.text((95,210+i*54),line,font=font,fill='black')
scan.save(root/'scan.png')
scan_pdf=canvas.Canvas(str(root/'scan.pdf'),pagesize=(595.28,841.89))
scan_pdf.drawImage(ImageReader(scan),0,0,width=595.28,height=841.89)
scan_pdf.save()
print(root)
