/**
 * Rasterise the MasLight monogram into every icon the app needs.
 *
 * The mark is drawn from the same geometry as assets/brand/mark.svg using
 * signed distance fields, so the PNGs and the SVG are the same shape rather
 * than two drawings that drift apart. PNG and ICO are written by hand: the
 * whole encoder is a hundred lines of zlib and CRC, which is a better trade
 * than a native image dependency in the build.
 *
 *   node scripts/make-icons.mjs
 */

import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// --- geometry, in the 256 unit space of the SVG ------------------------------

const STROKE = 13; // half of stroke-width 26
const M_PATH = [
  [56, 188, 56, 72],
  [56, 72, 128, 142],
  [128, 142, 200, 72],
  [200, 72, 200, 188],
];
const DOTS = [
  [56, 220, 12, [0xff, 0x4d, 0x4d]],
  [128, 220, 12, [0x4d, 0xff, 0x9e]],
  [200, 220, 12, [0x4d, 0xa6, 0xff]],
];
const INK = [0x0b, 0x0d, 0x10];
const FOREGROUND = [0xf5, 0xf7, 0xfa];
const CORNER = 56;

function sdSegment(px, py, ax, ay, bx, by) {
  const pax = px - ax;
  const pay = py - ay;
  const bax = bx - ax;
  const bay = by - ay;
  const h = Math.max(0, Math.min(1, (pax * bax + pay * bay) / (bax * bax + bay * bay)));
  const dx = pax - bax * h;
  const dy = pay - bay * h;
  return Math.hypot(dx, dy);
}

function sdRoundedBox(px, py, size, radius) {
  const half = size / 2;
  const qx = Math.abs(px - half) - (half - radius);
  const qy = Math.abs(py - half) - (half - radius);
  const outside = Math.hypot(Math.max(qx, 0), Math.max(qy, 0));
  return outside + Math.min(Math.max(qx, qy), 0) - radius;
}

/** Coverage of a shape whose signed distance is `d`, in pixels. */
function coverage(d) {
  return Math.max(0, Math.min(1, 0.5 - d));
}

function over(dst, src, alpha) {
  return [
    src[0] * alpha + dst[0] * (1 - alpha),
    src[1] * alpha + dst[1] * (1 - alpha),
    src[2] * alpha + dst[2] * (1 - alpha),
  ];
}

/**
 * Render the mark at `size` pixels.
 * `variant` is "app" (dark rounded tile) or "mono" (transparent, one colour).
 */
function render(size, variant = "app", monoColor = FOREGROUND) {
  const scale = size / 256;
  const rgba = Buffer.alloc(size * size * 4);

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      // Sample at the pixel centre, in SVG units.
      const sx = (x + 0.5) / scale;
      const sy = (y + 0.5) / scale;
      // Distances are in SVG units; convert to pixels so antialiasing is one
      // pixel wide at every size.
      const toPx = scale;

      let color = [0, 0, 0];
      let alpha = 0;

      if (variant === "app") {
        const bg = coverage(sdRoundedBox(sx, sy, 256, CORNER) * toPx);
        if (bg > 0) {
          color = INK;
          alpha = bg;
        }
      }

      // The M.
      let dM = Infinity;
      for (const [ax, ay, bx, by] of M_PATH) {
        dM = Math.min(dM, sdSegment(sx, sy, ax, ay, bx, by) - STROKE);
      }
      const mCoverage = coverage(dM * toPx);
      if (mCoverage > 0) {
        const ink = variant === "mono" ? monoColor : FOREGROUND;
        color = over(color, ink, mCoverage);
        alpha = alpha + mCoverage * (1 - alpha);
      }

      // The three LEDs.
      for (const [cx, cy, r, dotColor] of DOTS) {
        const d = Math.hypot(sx - cx, sy - cy) - r;
        const c = coverage(d * toPx);
        if (c > 0) {
          const ink = variant === "mono" ? monoColor : dotColor;
          color = over(color, ink, c);
          alpha = alpha + c * (1 - alpha);
        }
      }

      const p = (y * size + x) * 4;
      rgba[p] = Math.round(color[0]);
      rgba[p + 1] = Math.round(color[1]);
      rgba[p + 2] = Math.round(color[2]);
      rgba[p + 3] = Math.round(Math.max(0, Math.min(1, alpha)) * 255);
    }
  }
  return rgba;
}

// --- PNG ---------------------------------------------------------------------

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const out = Buffer.alloc(data.length + 12);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, "ascii");
  data.copy(out, 8);
  const crcInput = Buffer.concat([Buffer.from(type, "ascii"), data]);
  out.writeUInt32BE(crc32(crcInput), data.length + 8);
  return out;
}

function encodePng(rgba, size) {
  // One filter byte per scanline; filter 0 is "none", which compresses well
  // for flat art like this.
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    rgba.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // colour type: RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// --- ICO ---------------------------------------------------------------------

/** A single ICO entry as a bottom-up BGRA DIB, which every Windows accepts. */
function dibEntry(rgba, size) {
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0);
  header.writeInt32LE(size, 4);
  header.writeInt32LE(size * 2, 8); // colour data plus the AND mask
  header.writeUInt16LE(1, 12);
  header.writeUInt16LE(32, 14);
  header.writeUInt32LE(size * size * 4, 20);

  const pixels = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) {
    const src = (size - 1 - y) * size * 4;
    for (let x = 0; x < size; x++) {
      const s = src + x * 4;
      const d = (y * size + x) * 4;
      pixels[d] = rgba[s + 2];
      pixels[d + 1] = rgba[s + 1];
      pixels[d + 2] = rgba[s];
      pixels[d + 3] = rgba[s + 3];
    }
  }
  // The AND mask is unused for 32-bit icons but must still be present.
  const maskStride = Math.ceil(size / 32) * 4;
  const mask = Buffer.alloc(maskStride * size);
  return Buffer.concat([header, pixels, mask]);
}

function encodeIco(images) {
  const header = Buffer.alloc(6 + images.length * 16);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);

  let offset = header.length;
  const bodies = [];
  images.forEach((img, i) => {
    const body = dibEntry(img.rgba, img.size);
    const e = 6 + i * 16;
    header[e] = img.size >= 256 ? 0 : img.size;
    header[e + 1] = img.size >= 256 ? 0 : img.size;
    header[e + 2] = 0;
    header[e + 3] = 0;
    header.writeUInt16LE(1, e + 4);
    header.writeUInt16LE(32, e + 6);
    header.writeUInt32LE(body.length, e + 8);
    header.writeUInt32LE(offset, e + 12);
    offset += body.length;
    bodies.push(body);
  });
  return Buffer.concat([header, ...bodies]);
}

// --- output ------------------------------------------------------------------

const icons = join(root, "app", "src-tauri", "icons");
const brand = join(root, "assets", "brand");
const publicDir = join(root, "app", "public");
for (const dir of [icons, brand, publicDir]) mkdirSync(dir, { recursive: true });

const write = (path, buf) => {
  writeFileSync(path, buf);
  console.log(`  ${path.replace(root, ".")}  ${(buf.length / 1024).toFixed(1)} kB`);
};

console.log("MasLight icons");

for (const size of [32, 128, 256, 512]) {
  const rgba = render(size, "app");
  const name = size === 256 ? "128x128@2x.png" : size === 512 ? "icon.png" : `${size}x${size}.png`;
  write(join(icons, name), encodePng(rgba, size));
}

// Store icons for the bundlers that ask for them.
write(join(icons, "Square150x150Logo.png"), encodePng(render(150, "app"), 150));
write(join(icons, "Square44x44Logo.png"), encodePng(render(44, "app"), 44));
write(join(icons, "StoreLogo.png"), encodePng(render(50, "app"), 50));

// The Windows executable icon.
write(
  join(icons, "icon.ico"),
  encodeIco([16, 32, 48, 64, 128, 256].map((size) => ({ size, rgba: render(size, "app") })))
);

// Tray icons: monochrome and transparent, one per theme.
write(join(icons, "tray-light.png"), encodePng(render(32, "mono", [0x1a, 0x1d, 0x22]), 32));
write(join(icons, "tray-dark.png"), encodePng(render(32, "mono", [0xf5, 0xf7, 0xfa]), 32));

// Brand assets and the web favicon.
write(join(brand, "mark-512.png"), encodePng(render(512, "app"), 512));
write(join(publicDir, "favicon.png"), encodePng(render(64, "app"), 64));

console.log("done");
