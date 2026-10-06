// Deterministic application mark; no image service or build-time native dependency.
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';

function crc32(data) {
  let crc = 0xffffffff;
  for (const byte of data) {
    crc ^= byte;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
  }
  return (crc ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const name = Buffer.from(type), len = Buffer.alloc(4), crc = Buffer.alloc(4);
  len.writeUInt32BE(data.length); crc.writeUInt32BE(crc32(Buffer.concat([name, data])));
  return Buffer.concat([len, name, data, crc]);
}
function png(size) {
  const header = Buffer.alloc(13); header.writeUInt32BE(size); header.writeUInt32BE(size, 4); header[8] = 8; header[9] = 6;
  const raw = Buffer.alloc((size * 4 + 1) * size);
  // Match the shared monochrome pulse mark. Supersampling keeps the 32px tray icon crisp.
  const points = [[.16,.5],[.33,.5],[.43,.24],[.57,.76],[.67,.5],[.84,.5]];
  const distance = (x,y,a,b) => {
    const dx=b[0]-a[0],dy=b[1]-a[1];
    const t=Math.max(0,Math.min(1,((x-a[0])*dx+(y-a[1])*dy)/(dx*dx+dy*dy)));
    return Math.hypot(x-a[0]-t*dx,y-a[1]-t*dy);
  };
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    let alpha=0,shade=0;
    for(let sy=0;sy<4;sy++) for(let sx=0;sx<4;sx++) {
      const nx=(x+(sx+.5)/4)/size,ny=(y+(sy+.5)/4)/size;
      const qx=Math.abs(nx-.5)-.30,qy=Math.abs(ny-.5)-.30;
      const edge=Math.hypot(Math.max(qx,0),Math.max(qy,0))+Math.min(Math.max(qx,qy),0)-.17;
      if(edge>0)continue;
      const pulse=points.slice(1).some((p,i)=>distance(nx,ny,points[i],p)<.023);
      const value=pulse?54:edge>-.012?201:Math.round(253-ny*19);
      alpha+=255;shade+=value;
    }
    const value=alpha===0?0:Math.round(shade/(alpha/255));
    raw.set([value,value,value,Math.round(alpha/16)],y*(size*4+1)+1+x*4);
  }
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]), chunk('IHDR', header), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
mkdirSync('src-tauri/icons', { recursive: true });
for (const size of [32,128]) writeFileSync(`src-tauri/icons/${size}x${size}.png`, png(size));
const image = png(256), ico = Buffer.alloc(22);
ico.writeUInt16LE(1, 2); ico.writeUInt16LE(1, 4); ico.writeUInt16LE(1, 10); ico.writeUInt16LE(32, 12); ico.writeUInt32LE(image.length, 14); ico.writeUInt32LE(22, 18);
writeFileSync('src-tauri/icons/icon.ico', Buffer.concat([ico, image]));
