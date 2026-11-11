#!/usr/bin/env python3
"""Write each 1-bit CCITT G4 image in a PDF as a standalone TIFF file.

Used to read the scanned Table VI pages of Odeh (2004) at full resolution
(the PDF has no text layer for the table). Standard library only:

    python -I tools/fixtures/pdf_ccitt_to_tiff.py New_Criterion_Crescent.pdf outdir/

Files are named p<object number>.tif; in that PDF objects 20, 26, ..., 116
are journal pages 44 to 60. Any TIFF viewer can open them.
"""
import re,sys,struct,os
data=open(sys.argv[1],'rb').read(); out=sys.argv[2]
for m in re.finditer(rb'(\d+) 0 obj\r?<<(/BitsPerComponent 1.*?/Subtype/Image.*?)>>stream\r\n',data,re.S):
    d=m.group(2); num=int(m.group(1))
    W=int(re.search(rb'/Width (\d+)',d).group(1)); H=int(re.search(rb'/Height (\d+)',d).group(1))
    L=int(re.search(rb'/Length (\d+)',d).group(1))
    raw=data[m.end():m.end()+L]
    assert b'/K -1' in d and b'BlackIs1' not in d, d
    tags=[(256,4,W),(257,4,H),(258,3,1),(259,3,4),(262,3,0),(273,4,0),(277,3,1),(278,4,H),(279,4,len(raw)),(293,4,0)]
    n=len(tags); ifd_off=8; data_off=8+2+n*12+4
    b=b'II*\x00'+struct.pack('<I',ifd_off)+struct.pack('<H',n)
    for t,typ,v in tags:
        if t==273: v=data_off
        if typ==3: b+=struct.pack('<HHIHH',t,typ,1,v,0)
        else: b+=struct.pack('<HHII',t,typ,1,v)
    b+=struct.pack('<I',0)+raw
    open(os.path.join(out,f'p{num:03d}.tif'),'wb').write(b)
    print(num,W,H)
