/**
 * Turning a photograph into something the vision code can read.
 *
 * The webview already has a JPEG and PNG decoder, so the conversion happens
 * here and Rust receives a plain greyscale plane. That keeps an image decoding
 * dependency out of the application and sends about a quarter of the bytes a
 * base64 JPEG would.
 */

export interface LumaPhoto {
  /** Base64 of one byte per pixel, row major. */
  luma: string;
  width: number;
  height: number;
  /** An object URL for showing the photograph, to be revoked by the caller. */
  url: string;
  name: string;
}

/** Longest side after downscaling. */
const MAX_SIDE = 1280;

/**
 * Decode, downscale and reduce to luminance.
 *
 * Downscaling is not only about size: a phone photograph of a strip is mostly
 * empty space, and a smaller image makes the connected component pass cheaper
 * without losing a blob that is several pixels across to begin with.
 */
export async function toLuma(file: File): Promise<LumaPhoto> {
  const bitmap = await createImageBitmap(file);
  const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
  const width = Math.max(1, Math.round(bitmap.width * scale));
  const height = Math.max(1, Math.round(bitmap.height * scale));

  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) throw new Error("this browser cannot read image data");
  context.drawImage(bitmap, 0, 0, width, height);
  bitmap.close();

  const rgba = context.getImageData(0, 0, width, height).data;
  const luma = new Uint8Array(width * height);
  for (let i = 0; i < luma.length; i++) {
    const p = i * 4;
    // Rec.709 luminance, the same weights the colour pipeline uses.
    luma[i] =
      (rgba[p] * 0.2126 + rgba[p + 1] * 0.7152 + rgba[p + 2] * 0.0722) | 0;
  }

  return {
    luma: toBase64(luma),
    width,
    height,
    url: URL.createObjectURL(file),
    name: file.name,
  };
}

/** Base64 in chunks, because a whole plane blows the argument limit. */
function toBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}
