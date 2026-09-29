// Run: npm install --prefix target/icon-tools sharp
//      node scripts/build-icons.mjs [dependency-directory]
// Cargo builds use checked-in icons and do not need Node or sharp.
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const dependencies = resolve(process.argv[2] ?? fileURLToPath(new URL('../target/icon-tools/', import.meta.url)));
const sharp = createRequire(resolve(dependencies, 'package.json'))('sharp');
const output = fileURLToPath(new URL('../assets/brand/', import.meta.url));
await mkdir(output, { recursive: true });
const mark = await readFile(resolve(output, 'markdown-note.svg'));
const png = (size) => sharp(mark, { density: 1536 }).resize(size, size).png().toBuffer();
await writeFile(resolve(output, 'markdown-note.png'), await png(512));

const sizes = [16, 24, 32, 48, 64, 128, 256];
const images = await Promise.all(sizes.map(png));
const header = Buffer.alloc(6 + 16 * sizes.length);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
images.forEach((image, index) => {
  const entry = 6 + index * 16;
  header[entry] = sizes[index] % 256;
  header[entry + 1] = sizes[index] % 256;
  header.writeUInt16LE(1, entry + 4);
  header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(image.length, entry + 8);
  header.writeUInt32LE(offset, entry + 12);
  offset += image.length;
});
await writeFile(resolve(output, 'markdown-note.ico'), Buffer.concat([header, ...images]));

// Modern macOS ICNS entries contain PNG images, including Retina resolutions.
const chunks = await Promise.all(
  [['icp4', 16], ['icp5', 32], ['icp6', 64], ['ic07', 128], ['ic08', 256],
   ['ic09', 512], ['ic10', 1024], ['ic11', 32], ['ic12', 64],
   ['ic13', 256], ['ic14', 512]].map(async ([type, size]) => {
    const image = await png(size);
    const chunk = Buffer.alloc(8);
    chunk.write(type);
    chunk.writeUInt32BE(image.length + 8, 4);
    return Buffer.concat([chunk, image]);
  }),
);
const icns = Buffer.alloc(8);
icns.write('icns');
icns.writeUInt32BE(8 + chunks.reduce((sum, chunk) => sum + chunk.length, 0), 4);
await writeFile(resolve(output, 'markdown-note.icns'), Buffer.concat([icns, ...chunks]));
console.log('Generated desktop icons from assets/brand/markdown-note.svg.');
