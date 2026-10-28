import {mkdir,writeFile,readFile,copyFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const downloads=path.join(root,'.tools','downloads');await mkdir(downloads,{recursive:true});
await mkdir(path.join(root,'tools'),{recursive:true});
const engines=[
 {name:'pandoc',version:'3.12.1',url:'https://github.com/jgm/pandoc/releases/download/3.12.1/pandoc-3.12.1-windows-x86_64.zip',sha256:'6ef431cb20b2c24a0371fc19200dc49d452be54b1b5c9239f3462841ef53e587'},
 {name:'typst',version:'0.15.1',url:'https://github.com/typst/typst/releases/download/v0.15.1/typst-x86_64-pc-windows-msvc.zip',sha256:'19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736'},
];
for(const e of engines){
 const target=path.join(downloads,e.name+'.zip');let bytes;
 try{bytes=await readFile(target);}catch{}
 if(!bytes||createHash('sha256').update(bytes).digest('hex')!==e.sha256){const r=await fetch(e.url);if(!r.ok)throw Error(e.name+': HTTP '+r.status);bytes=Buffer.from(await r.arrayBuffer());}
 if(createHash('sha256').update(bytes).digest('hex')!==e.sha256)throw Error(e.name+': checksum mismatch');
 await writeFile(target,bytes);console.log(e.name,e.version,bytes.length,'bytes, verified');
}
await writeFile(path.join(root,'tools','engine-manifest.json'),JSON.stringify({acquired:new Date().toISOString(),engines},null,2));
