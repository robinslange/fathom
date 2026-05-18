const PATTERNS: RegExp[] = [
  /^\s*Other information and formats\s*:\s*www\.gutenberg\.org\/ebooks\/\d+/i,
  /^\s*This eBook was prepared by\b/i,
  /^\s*Contributor\s*:\s*/i,
  /^\s*Author of introduction,?\s*etc\.\s*:\s*/i,
  /^\s*\*\*\*\s*(END|START) OF (THE|THIS)?\s*PROJECT GUTENBERG EBOOK\b/i,
  /^\s*The Project Gutenberg (License|eBook)\b/i,
];

export function isBoilerplate(text: string): boolean {
  const t = text.trim();
  if (t.length === 0) return false;
  return PATTERNS.some((re) => re.test(t));
}
