/** Convert #rrggbb (or rgb/rgba text) to rgba with the given alpha. */
export function alpha(color: string, a: number): string {
  const c = color.trim();
  if (c.startsWith('#') && (c.length === 7 || c.length === 4)) {
    const full = c.length === 4 ? `#${c[1]}${c[1]}${c[2]}${c[2]}${c[3]}${c[3]}` : c;
    const n = parseInt(full.slice(1), 16);
    return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${a})`;
  }
  const m = c.match(/rgba?\(([^)]+)\)/);
  if (m?.[1]) {
    const [r, g, b] = m[1].split(',').map((x) => x.trim());
    return `rgba(${r}, ${g}, ${b}, ${a})`;
  }
  return c;
}
