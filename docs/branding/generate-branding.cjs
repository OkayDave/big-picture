// Rebuild: node generate-branding.cjs <Inter-SemiBold.ttf> <node_modules directory>
// Rendering tools: npm install --prefix <tools directory> @resvg/resvg-js opentype.js
// The wordmark is outlined; the exported SVGs do not require an installed font.
const fs = require('node:fs');
const path = require('node:path');
const [fontFile, modules] = process.argv.slice(2);
if (!fontFile || !modules) throw new Error('Provide the font file and rendering node_modules directory.');
const { Resvg } = require(path.resolve(modules, '@resvg/resvg-js'));
const opentype = require(path.resolve(modules, 'opentype.js'));
const font = opentype.parse(fs.readFileSync(fontFile));
const out = __dirname;
const mint = '#70E0C0', ink = '#111A22', paper = '#F4F3ED';

function mark(base = ink, accent = mint) {
  return `<path fill="${base}" fill-rule="evenodd" d="M16 96H160V208H240V240H16ZM48 128V208H128V128Z"/><path fill="${accent}" d="M80 16H240V208H208V48H80Z"/>`;
}
function svg(width, height, body, label) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="${label}"><title>${label}</title>${body}</svg>\n`;
}
function write(name, data) { fs.writeFileSync(path.join(out, name), data); }
function png(source, width) {
  return new Resvg(source, { fitTo: { mode: 'width', value: width } }).render().asPng();
}
const word = new opentype.Path();
let penX = 324, previous;
for (const character of 'Big Picture') {
  const glyph = font.charToGlyph(character);
  if (previous) penX += font.getKerningValue(previous, glyph) * 92 / font.unitsPerEm;
  word.extend(glyph.getPath(penX, 192, 92));
  penX += glyph.advanceWidth * 92 / font.unitsPerEm;
  previous = glyph;
}
const wordPath = word.toPathData(3);
const width = Math.ceil(word.getBoundingBox().x2 + 40);
function lockup(base) {
  return svg(width, 320, `<g transform="translate(24 32)">${mark(base)}</g><path fill="${base}" d="${wordPath}"/>`, 'Big Picture');
}
const light = lockup(ink), dark = lockup(paper);
write('big-picture-logo-light.svg', light);
write('big-picture-logo-dark.svg', dark);
write('big-picture-logo-light.png', png(light, width * 2));
write('big-picture-logo-dark.png', png(dark, width * 2));
for (const [name, base, accent] of [
  ['colour', ink, mint], ['reversed', paper, mint],
  ['mono-dark', ink, ink], ['mono-light', paper, paper],
]) {
  write(`big-picture-mark-${name}.svg`, svg(256, 256, mark(base, accent), 'Big Picture expanding frame symbol'));
}
const icon = svg(512, 512, `<rect width="512" height="512" rx="104" fill="${ink}"/><g transform="translate(64 64) scale(1.5)">${mark(paper)}</g>`, 'Big Picture app icon');
write('big-picture-app-icon.svg', icon);
const sizes = [16, 24, 32, 48, 64, 128, 256, 512, 1024];
const rendered = new Map(sizes.map(size => [size, png(icon, size)]));
for (const size of [32, 256, 512, 1024]) write(`big-picture-app-icon-${size}.png`, rendered.get(size));
const icoSizes = sizes.filter(s => s <= 256);
const header = Buffer.alloc(6 + icoSizes.length * 16);
header.writeUInt16LE(1, 2); header.writeUInt16LE(icoSizes.length, 4);
let offset = header.length;
icoSizes.forEach((size, i) => {
  const data = rendered.get(size), entry = 6 + i * 16;
  header[entry] = size === 256 ? 0 : size;
  header[entry + 1] = size === 256 ? 0 : size;
  header.writeUInt16LE(1, entry + 4); header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(data.length, entry + 8); header.writeUInt32LE(offset, entry + 12);
  offset += data.length;
});
write('big-picture-app-icon.ico', Buffer.concat([header, ...icoSizes.map(s => rendered.get(s))]));

// Review on both backgrounds, including true small-size icon specimens.
const fit = Math.min(1, 1080 / width);
const specimens = [16, 24, 32, 48, 64].map((size, i) => `<image x="${900 + i * 74}" y="${504 + (64 - size) / 2}" width="${size}" height="${size}" href="data:image/png;base64,${rendered.get(size).toString('base64')}"/>`).join('');
const preview = svg(1440, 880,
  `<rect width="1440" height="440" fill="${paper}"/><rect y="440" width="1440" height="440" fill="${ink}"/>` +
  `<g transform="translate(70 60) scale(${fit})">${light.match(/<g[\s\S]*<\/svg>/)[0].replace('</svg>', '')}</g>` +
  `<g transform="translate(70 500) scale(${fit})">${dark.match(/<g[\s\S]*<\/svg>/)[0].replace('</svg>', '')}</g>` +
  `<image x="1150" y="110" width="180" height="180" href="data:image/png;base64,${rendered.get(512).toString('base64')}"/>` +
  `<g transform="translate(1120 625) scale(.65)">${mark(paper, paper)}</g>`, 'Big Picture Expand logo on light and dark backgrounds');
// Small sizes get their own sheet so the main logo preview remains uncluttered.
write('big-picture-logo-preview.png', png(preview, 1440));
const smallPreview = svg(1440, 660, `<rect width="1440" height="660" fill="${paper}"/><image x="72" y="74" width="512" height="512" href="data:image/png;base64,${rendered.get(512).toString('base64')}"/>${specimens}<g transform="translate(850 100) scale(1.35)">${mark()}</g>`, 'Big Picture app icon and small-size checks');
write('big-picture-icon-preview.png', png(smallPreview, 1440));
console.log(`Exported outlined logos, symbols, PNGs, a seven-size Windows icon, and two previews to ${out}`);
