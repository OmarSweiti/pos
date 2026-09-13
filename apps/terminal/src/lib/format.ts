/**
 * Display formatting for the register's screens.
 *
 * Three rules from the plan shape this file, and each one is a decision the
 * layers below deliberately refused to make:
 *
 * **The exponent belongs to the currency (I-2).** `packages/money`'s
 * `formatMinor` already renders exact minor units — digits only, no grouping,
 * no symbol — and its own docstring says why: "both are locale decisions and
 * this register runs in Arabic by default (§10). The locale-aware wrapper
 * belongs in the UI layer, over this exact string." This module is that
 * wrapper. It never re-derives an exponent and never divides by 100.
 *
 * **Western Arabic digits, in both locales.** Arabic is the product rather
 * than a translation (§10), but a Jordanian minimarket writes prices in
 * `0`–`9`, not `٠`–`٩`. That is a decision rather than an omission, so it is
 * recorded in `NUMBER_SHAPE` where it can be read and changed, instead of
 * being the accidental consequence of never calling `Intl`.
 *
 * **Never `toLocaleString` inline.** Not a style preference: a call site that
 * reaches for it gets the host's default numbering system, which under an
 * `ar` locale is Arabic-Indic. Centralising the locale here is what makes the
 * digit rule enforceable at one place rather than at every call site.
 *
 * No float touches an amount here (I-1). Every arithmetic operation below is
 * on `bigint`, and a value that is not a whole number of minor or milli units
 * is refused rather than rounded — rounding it would hide the upstream defect
 * at the moment it starts costing money.
 */

import { type Currency, formatMinor, type MinorUnits } from "@pos/money";
import { DEFAULT_LOCALE, type Locale } from "./direction";

/** Milli-units per unit (I-3). A quantity of `1` is stored as `1000`. */
const MILLI = 1000n;

/** Fractional digits a weighed quantity carries: grams, for a trade scale. */
const WEIGHED_DIGITS = 3;

/** A refusal from this module, distinct from `MoneyError` raised below it. */
export class FormatError extends Error {}

/**
 * How a number is written, per locale.
 *
 * Both rows are identical today, and that is the point: the table is the
 * record that Arabic keeps Western Arabic digits and a `.` decimal mark here,
 * rather than that nobody considered it. A future locale that genuinely
 * differs edits this table and nothing else.
 */
interface NumberShape {
  /** Thousands separator, applied to the integer part only. */
  readonly group: string;
  /** Decimal mark. */
  readonly decimal: string;
}

const NUMBER_SHAPE: Record<Locale, NumberShape> = {
  ar: { group: ",", decimal: "." },
  en: { group: ",", decimal: "." },
};

/**
 * The Unicode numbering-system key forced onto every `Intl` formatter here.
 *
 * This pin is load-bearing, and not in the way it first looks. Bare `ar`
 * happens to resolve to `latn` on current ICU, so a test that only ever passes
 * `"ar"` would pass without it. `ar-JO` does not — this register's own
 * region resolves to `arab` and renders ٢٠٢٦ and ٠٠:٣٠. So does `ar-EG`. Measured on
 * node v24.19.0, full ICU.
 *
 * Regionalising `Locale` to `ar-JO` is the obvious next edit for a Jordanian
 * product, and it is exactly the edit that would silently switch every date on
 * every receipt to digits the plan forbids. Pinning the numbering system means
 * the digit rule survives that change rather than depending on which tag the
 * caller happened to pass.
 */
const LATIN_NUMERALS = "latn";

/** Group the integer part in threes. The fraction is never grouped. */
function grouped(whole: string, separator: string): string {
  return whole.replace(/\B(?=(\d{3})+(?!\d))/g, separator);
}

/**
 * Apply a locale's number shape to the plain `-?ddd(.ddd)?` string the money
 * package produces. Digits are carried through untouched, which is the digit
 * rule: no transliteration happens anywhere in this module.
 */
function shaped(plain: string, locale: Locale): string {
  const { group, decimal } = NUMBER_SHAPE[locale];
  const negative = plain.startsWith("-");
  const body = negative ? plain.slice(1) : plain;
  const [whole, fraction] = body.split(".");
  const joined =
    fraction === undefined
      ? grouped(whole, group)
      : `${grouped(whole, group)}${decimal}${fraction}`;
  return negative ? `-${joined}` : joined;
}

/**
 * A transaction amount, at the currency's full exponent.
 *
 * Every fils is shown. A total, a line price, a tender and a change amount all
 * use this: a receipt that rounds a displayed total away from what was charged
 * is the defect I-1 exists to prevent, and it is invisible in testing because
 * both numbers look plausible.
 */
export function formatMoney(
  minor: MinorUnits,
  currency: Currency,
  locale: Locale = DEFAULT_LOCALE,
): string {
  return shaped(formatMinor(minor, currency), locale);
}

/**
 * A catalogue or shelf amount, shortened only where shortening is exact.
 *
 * The plan allows "a shorter catalogue display ... only when exact", so this
 * drops trailing zeros and nothing else: `2.500` becomes `2.5` and `2.000`
 * becomes `2`, because neither hides a fils. `2.505` is returned whole,
 * because the alternative is a shelf label that disagrees with the till by
 * five fils — the exact class of error that makes a customer right and the
 * merchant wrong at the counter.
 */
export function formatCatalogMoney(
  minor: MinorUnits,
  currency: Currency,
  locale: Locale = DEFAULT_LOCALE,
): string {
  const exact = formatMinor(minor, currency);
  const point = exact.indexOf(".");
  if (point === -1) return shaped(exact, locale);

  const whole = exact.slice(0, point);
  const trimmed = exact.slice(point + 1).replace(/0+$/, "");
  return shaped(trimmed === "" ? whole : `${whole}.${trimmed}`, locale);
}

/** Widen a milli-unit quantity to `bigint`, refusing anything inexact (I-3). */
function wholeMilli(milli: MinorUnits): bigint {
  if (typeof milli === "bigint") return milli;
  if (!Number.isInteger(milli)) {
    throw new FormatError(
      `a quantity must be whole milli-units (I-3), got ${milli}. ` +
        "A fraction here means a float touched this quantity upstream.",
    );
  }
  if (!Number.isSafeInteger(milli)) {
    throw new FormatError(
      `${milli} is past Number.MAX_SAFE_INTEGER and is no longer an exact i64.`,
    );
  }
  return BigInt(milli);
}

/**
 * A quantity, from milli-units.
 *
 * Weighed and discrete goods share one representation (I-3), so the caller
 * says which this is rather than the value implying it — `1000` is both "one
 * loaf" and "one kilogram", and only the product knows.
 *
 * A discrete quantity that is not a whole number of units is a defect
 * upstream, and it is shown at full precision rather than rounded to a tidy
 * integer. Displaying `2` for `1500` would make the wrong number look right.
 */
export function formatQty(milli: MinorUnits, weighed: boolean): string {
  const value = wholeMilli(milli);
  const negative = value < 0n;
  const magnitude = negative ? -value : value;
  const sign = negative ? "-" : "";

  const whole = magnitude / MILLI; // exact: bigint division truncates
  const rest = magnitude % MILLI;

  if (!weighed && rest === 0n) return `${sign}${whole}`;
  return `${sign}${whole}.${rest.toString().padStart(WEIGHED_DIGITS, "0")}`;
}

/**
 * An instant, in a named zone, as `YYYY-MM-DD HH:mm`.
 *
 * The layout is fixed rather than locale-ordered on purpose. A business date
 * on a shift report or an audit row is read against other records and against
 * the fiscal documents, so it must not reorder between `ar` and `en`; the
 * locale's job here is the numbering system, which is forced to `latn`.
 *
 * The zone is an argument. Neither the host's clock nor the host's zone is
 * read, which is the same discipline `pos-domain` applies in Rust (I-8) and
 * what lets a test assert a Jordanian business date from any machine.
 */
export function formatDate(
  iso: string,
  tz: string,
  locale: Locale = DEFAULT_LOCALE,
): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) {
    throw new FormatError(`not an ISO-8601 instant: ${iso}`);
  }

  let parts: Intl.DateTimeFormatPart[];
  try {
    parts = new Intl.DateTimeFormat(`${locale}-u-nu-${LATIN_NUMERALS}`, {
      timeZone: tz,
      hourCycle: "h23",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).formatToParts(at);
  } catch {
    // `Intl` throws RangeError for an unknown zone. A named refusal beats a
    // silent fall back to UTC, which would date a sale to the wrong business
    // day either side of midnight.
    throw new FormatError(`unknown IANA time zone: ${tz}`);
  }

  const part = (type: Intl.DateTimeFormatPartTypes): string =>
    parts.find((candidate) => candidate.type === type)?.value ?? "";

  return `${part("year")}-${part("month")}-${part("day")} ${part("hour")}:${part("minute")}`;
}
