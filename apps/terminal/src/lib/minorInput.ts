/**
 * Reading money out of a text input, without letting a float in.
 *
 * `packages/money` exports `toMinor`, which refuses a `number` that is not a
 * whole number of minor units — that refusal is where invariant I-1 ("no float
 * touches money") is enforced on the TypeScript side. It only helps if callers
 * use it. The first money input in this application did not: it wrote
 * `Number(e.target.value)` straight into the store, so `1.5` became a fractional
 * "minor unit" amount that crossed IPC toward an `i64`, and the guard written
 * for exactly that case never ran.
 *
 * A DOM `<input type="number">` cannot be trusted to prevent it. Its `value` is
 * a string, it accepts `1.5`, `1e3` and `-0`, and on a browser with a comma
 * decimal separator it accepts more still. So the string is parsed here and the
 * store only ever receives a value `toMinor` accepted.
 */

import { MoneyError, toMinor } from "@pos/money";

/** A refusal carries a reason, because a silently ignored keystroke reads as a broken field. */
export type MinorInput =
  | { readonly ok: true; readonly value: number }
  | { readonly ok: false; readonly reason: string };

/**
 * Parse minor units typed by a person.
 *
 * Empty input is a refusal rather than zero: a cleared field means "not decided
 * yet", and treating it as a total of zero is how a mis-tender happens.
 */
export function parseMinorInput(raw: string): MinorInput {
  const text = raw.trim();
  if (text === "") return { ok: false, reason: "empty" };

  const parsed = Number(text);
  if (!Number.isFinite(parsed)) return { ok: false, reason: "not a number" };

  try {
    // `toMinor` returns bigint and refuses fractions and anything past
    // Number.MAX_SAFE_INTEGER. Narrowing back to `number` is safe only because
    // it already proved the value is an exact integer inside that range — the
    // IPC boundary carries a JSON number, not a bigint.
    return { ok: true, value: Number(toMinor(parsed)) };
  } catch (error) {
    if (error instanceof MoneyError) {
      return { ok: false, reason: "money must be whole minor units" };
    }
    throw error;
  }
}

/**
 * Parse a count — how many ways to split. Not money, so it has no minor-unit
 * rule, but it is still an integer and still refuses a fraction: asking for 2.5
 * tenders is not a smaller request, it is a malformed one.
 */
export function parseCountInput(raw: string, minimum = 1): MinorInput {
  const text = raw.trim();
  if (text === "") return { ok: false, reason: "empty" };

  const parsed = Number(text);
  if (!Number.isInteger(parsed))
    return { ok: false, reason: "must be a whole number" };
  if (parsed < minimum)
    return { ok: false, reason: `must be at least ${minimum}` };
  return { ok: true, value: parsed };
}
