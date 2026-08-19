/** Format integer minor units for display. Mirrors the Rust rule: ints only. */
export function formatMinor(minor: number, fractionDigits = 2): string {
  const divisor = 10 ** fractionDigits;
  const sign = minor < 0 ? "-" : "";
  const abs = Math.abs(minor);
  const units = Math.trunc(abs / divisor);
  const frac = String(abs % divisor).padStart(fractionDigits, "0");
  return `${sign}${units}.${frac}`;
}
