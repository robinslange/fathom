export const encoder = new TextEncoder();

export function utf8ByteLength(s: string): number {
  return encoder.encode(s).length;
}
