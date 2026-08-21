/**
 * Money for the TypeScript side of the register.
 *
 * Two invariants from `docs/implementation/01-conventions.md` §1 shape every
 * line of this file:
 *
 * **I-1 · no float touches money — in Rust, TypeScript, SQL or JSON.** Rust
 * enforces this with `clippy::float_arithmetic = "deny"`. TypeScript has no
 * equivalent lint, so the guard here is structural instead: every arithmetic
 * operation below is on `bigint`, where `/` is exact integer division and there
 * is no IEEE-754 anywhere to round for you. A `number` that is not an integer
 * is rejected at the door rather than silently truncated.
 *
 * **I-2 · the minor-unit exponent is per-currency data, never a constant.**
 * JOD is 3 (1 dinar = 1000 fils), not 2. So there is no default exponent and no
 * default currency: a caller must say which currency it is formatting, and the
 * type system makes that non-negotiable. The previous version of this function
 * defaulted to 2 decimal places, which silently rendered every JOD amount a
 * factor of ten wrong.
 */

/** A currency and its minor-unit exponent. Mirrors the Rust `Currency`. */
export interface Currency {
  /** ISO-4217 alphabetic code. */
  readonly code: string;
  /** Minor units per major unit, as a power of ten. JOD = 3. */
  readonly exponent: number;
}

/** The home currency. 1 dinar = 1000 fils, so the exponent is 3, never 2. */
export const JOD: Currency = { code: "JOD", exponent: 3 };
export const USD: Currency = { code: "USD", exponent: 2 };
export const EUR: Currency = { code: "EUR", exponent: 2 };

/** Amounts crossing IPC arrive as JSON numbers; `bigint` is accepted directly. */
export type MinorUnits = number | bigint;

export class MoneyError extends Error {}

/**
 * Widen an amount to `bigint`, refusing anything that is not a whole number of
 * minor units.
 *
 * This is where I-1 is actually enforced. A `number` carrying a fraction is the
 * fingerprint of float arithmetic upstream — a percentage applied in JavaScript,
 * a division that should have happened in `rust_decimal` — and rounding it here
 * would hide the bug at exactly the moment it starts costing money. `BigInt()`
 * also rejects anything past `Number.MAX_SAFE_INTEGER`, which is the other way a
 * JSON number quietly stops being an `i64`.
 */
export function toMinor(amount: MinorUnits): bigint {
  if (typeof amount === "bigint") return amount;
  if (!Number.isInteger(amount)) {
    throw new MoneyError(
      `money must be whole minor units (I-1), got ${amount}. ` +
        "A fraction here means a float touched this amount upstream.",
    );
  }
  if (!Number.isSafeInteger(amount)) {
    throw new MoneyError(
      `${amount} is past Number.MAX_SAFE_INTEGER and is no longer an exact i64. ` +
        "Carry this amount as a bigint.",
    );
  }
  return BigInt(amount);
}

/**
 * Render minor units as a decimal string for display.
 *
 * Digits only — no grouping separators and no currency symbol, because both are
 * locale decisions and this register runs in Arabic by default (§10). The
 * locale-aware wrapper belongs in the UI layer, over this exact string.
 */
export function formatMinor(amount: MinorUnits, currency: Currency): string {
  const exponent = currency.exponent;
  if (!Number.isInteger(exponent) || exponent < 0 || exponent > 6) {
    throw new MoneyError(
      `${currency.code} has an implausible minor-unit exponent (${exponent})`,
    );
  }

  const value = toMinor(amount);
  const negative = value < 0n;
  const magnitude = negative ? -value : value;
  const sign = negative ? "-" : "";

  // A zero-exponent currency has no minor unit at all, so no decimal point.
  if (exponent === 0) return `${sign}${magnitude}`;

  const divisor = 10n ** BigInt(exponent);
  const major = magnitude / divisor; // exact: bigint division truncates, never rounds
  const minor = magnitude % divisor;
  return `${sign}${major}.${minor.toString().padStart(exponent, "0")}`;
}
