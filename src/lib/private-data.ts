// Advisory client-side scan for private data a tester might paste into a
// finding or report by accident. This is a HELP, not a guarantee or a gate:
// it surfaces likely leaks (emails, tokens, IPs, long hex) so the tester can
// review before submitting. It cannot catch everything and is deliberately
// tuned for high signal over completeness — every false positive is friction.
//
// Findings are public by design (they're about a bug, not a person), so the
// right protection at this layer is to help authors avoid putting private
// data in public text in the first place. Cohort-encrypted attachments are a
// later layer (there is no attachment surface yet).

export interface PrivateDataHit {
  label: string;
  hint: string;
}

// Each pattern is intentionally conservative — better to miss an edge case
// than to cry wolf on ordinary testing prose.
const PATTERNS: { label: string; hint: string; re: RegExp }[] = [
  {
    label: "email address",
    hint: "looks like an email address",
    re: /\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b/,
  },
  {
    label: "IP address",
    hint: "looks like an IP address",
    // IPv4 only — high signal, low false-positive vs trying to catch IPv6.
    re: /\b(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b/,
  },
  {
    label: "API key or token",
    hint: "looks like an API key or token",
    // Only PREFIXED token shapes (sk-/pk-/ghp_/Bearer) — genuinely high
    // signal. Deliberately NOT matching bare long alphanumeric runs, which
    // would false-positive on Holochain agent keys and action hashes that
    // testers legitimately paste into bug reports.
    re: /\b(?:sk|pk|ghp|gho|ghs|xox[baprs])[-_][A-Za-z0-9]{16,}\b|\bBearer\s+[A-Za-z0-9._-]{20,}\b/,
  },
  {
    label: "long hex string",
    hint: "looks like a key, hash, or secret in hex",
    // 40+ hex chars (private keys, some tokens). Catches things a person
    // would not normally type by hand into a bug report.
    re: /\b[0-9a-fA-F]{40,}\b/,
  },
];

/**
 * Scan text for likely private-data patterns. Returns one hit per matched
 * category (deduplicated by label). Empty array = nothing flagged.
 */
export function scanForPrivateData(text: string): PrivateDataHit[] {
  if (!text) return [];
  const hits: PrivateDataHit[] = [];
  for (const p of PATTERNS) {
    if (p.re.test(text)) {
      hits.push({ label: p.label, hint: p.hint });
    }
  }
  return hits;
}
