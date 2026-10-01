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
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const i = y * (size * 4 + 1) + 1 + x * 4;
    const nx = x / size, ny = y / size;
    const bar = (nx > .21 && nx < .35 && ny > .50 && ny < .79) || (nx > .43 && nx < .57 && ny > .32 && ny < .79) || (nx > .65 && nx < .79 && ny > .17 && ny < .79);
    raw.set(bar ? [242, 242, 243, 255] : [8, 125, 199, 255], i);
  }
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]), chunk('IHDR', header), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
mkdirSync('src-tauri/icons', { recursive: true });
for (const size of [32,128]) writeFileSync(`src-tauri/icons/${size}x${size}.png`, png(size));
const image = png(256), ico = Buffer.alloc(22);
ico.writeUInt16LE(1, 2); ico.writeUInt16LE(1, 4); ico.writeUInt16LE(1, 10); ico.writeUInt16LE(32, 12); ico.writeUInt32LE(image.length, 14); ico.writeUInt32LE(22, 18);
writeFileSync('src-tauri/icons/icon.ico', Buffer.concat([ico, image]));
