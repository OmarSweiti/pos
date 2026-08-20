/**
 * Locale and writing direction.
 *
 * Conventions §10: Arabic is not a translation of this product — it is the
 * product, and English is the toggle. So `ar`/RTL is the default, and switching
 * flips `lang` and `dir` together and nothing else.
 *
 * The document root is an argument rather than something these functions reach
 * for. That is the same discipline `pos-domain` applies to clocks and IDs, and it
 * is what lets every rule below be tested without a browser.
 */

export type Locale = "ar" | "en";
export type Direction = "rtl" | "ltr";

/** §10: the register is right-to-left unless someone asks otherwise. */
export const DEFAULT_LOCALE: Locale = "ar";

/** The only part of a document element these functions touch. */
export interface DocumentRoot {
  lang: string;
  dir: string;
}

export function directionFor(locale: Locale): Direction {
  return locale === "ar" ? "rtl" : "ltr";
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
