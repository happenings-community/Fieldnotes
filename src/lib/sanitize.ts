/** Validate image URL to prevent XSS via javascript: or data: URIs */
export function sanitizeImageSrc(url: string | null): string | null {
  if (!url) return null;
  if (url.startsWith('blob:') || url.startsWith('data:image/')) return url;
  try {
    const parsed = new URL(url);
    if (parsed.protocol === 'https:' || parsed.protocol === 'http:') return url;
  } catch {}
  return null;
}
