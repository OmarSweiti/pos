/**
 * Locale and writing direction, as this register applies them.
 *
 * Conventions §10: Arabic is not a translation of this product — it is the
 * product, and English is the toggle. So `ar`/RTL is the default, and switching
 * flips `lang` and `dir` together and nothing else.
 *
 * **The rule itself now lives in `@pos/ui`** (microstep 1.11.1): which locales
 * exist, which one is the default and which direction each writes in are
 * product law rather than terminal law, and the back office renders the same
 * two languages. They are re-exported here so every call site in this app keeps
 * one import, and so `direction.test.ts` proves the binding rather than a
 * second copy of the answer.
 *
 * What stays is what is this app's own: applying a locale to a document root,
 * and the affordance that switches it.
 *
 * The document root is an argument rather than something these functions reach
 * for. That is the same discipline `pos-domain` applies to clocks and IDs, and it
 * is what lets every rule below be tested without a browser.
 */

import {
  DEFAULT_LOCALE,
  type Direction,
  directionFor,
  type Locale,
} from "@pos/ui";

export { DEFAULT_LOCALE, type Direction, directionFor, type Locale };

/** The only part of a document element these functions touch. */
export interface DocumentRoot {
  lang: string;
  dir: string;
}

/** The opposite of the current selection — what the toggle switches to. */
export function toggled(locale: Locale): Locale {
  return locale === "ar" ? "en" : "ar";
}

/**
 * Apply a locale to a document root. `lang` and `dir` move together: a root
 * where they disagree renders Arabic text left-to-right, or the reverse.
 */
export function applyLocale(root: DocumentRoot, locale: Locale): Locale {
  root.lang = locale;
  root.dir = directionFor(locale);
  return locale;
}

/** The label for the button that switches away from `locale`, in its own script. */
export function toggleLabel(locale: Locale): string {
  return toggled(locale) === "ar" ? "العربية" : "English";
}
