/**
 * The register's message catalogue and its boot-time locale (microstep 1.11.1,
 * gap G-5).
 *
 * Three things live here, and each is one half of a rule conventions §10 states
 * and nothing enforced until now:
 *
 *  1. **`messages`** — the two catalogues behind one lookup, so a component
 *     reads a key and never a literal.
 *  2. **`t`** — the only way to turn a key into text. It takes the locale as an
 *     argument rather than reading a store, which is what lets a renderer, a
 *     test or a future receipt preview resolve a string without a React tree.
 *  3. **`installLocale`** — the product-side RTL default.
 *
 * The third is the one worth explaining, because it looks redundant and is not.
 * `index.html` already ships `<html lang="ar" dir="rtl">`, and `1.11.0`'s test
 * harness feeds that very file to jsdom. But a served document is not the
 * product deciding anything: `useLocale` seeds `DEFAULT_LOCALE` into React
 * state and calls `applyLocale` only when somebody **toggles**, so before this
 * function existed the register's direction was whatever HTML happened to
 * arrive — an `index.html` regressed to `ltr`, a Tauri window served from
 * somewhere else, or a second entry point would all render Arabic
 * left-to-right with every test still green.
 *
 * `main.tsx` calls it before the first render, and
 * `arabic_is_the_rendered_default_on_a_document_that_says_otherwise` proves it
 * against a root that says the opposite.
 */

import { type Catalog, DEFAULT_LOCALE, type Locale } from "@pos/ui";
import { applyLocale, type DocumentRoot } from "../lib/direction";
import { ar, type MessageKey } from "./ar";
import { en } from "./en";

export type { MessageKey } from "./ar";

/** Both catalogues, keyed by locale. */
export const messages: Readonly<Record<Locale, Catalog<MessageKey>>> = {
  ar,
  en,
};

/**
 * The text for `key` in `locale`.
 *
 * There is no fallback to English and no fallback to the key. A missing key
 * cannot reach here — `MessageKey` is derived from the Arabic catalogue and
 * `Catalog<MessageKey>` holds the English one to it — so a runtime fallback
 * would only ever hide a catalogue that was assembled rather than written.
 */
export function t(locale: Locale, key: MessageKey): string {
  return messages[locale][key];
}

/**
 * Establish the register's writing direction on `root` before the first paint.
 *
 * Returns the locale it applied, so a caller can seed its own state from the
 * same decision rather than restating `DEFAULT_LOCALE`.
 *
 * It takes no locale. An optional `locale = DEFAULT_LOCALE` parameter was
 * written here first and removed: nothing passes one — the toggle goes through
 * `useLocale`, which calls `applyLocale` directly — so the mutation sweep
 * found it as an argument no test could distinguish from a constant. The step
 * that boots from a stored preference adds the parameter back with its caller
 * and its test, which is cheaper than carrying an untested one until then.
 */
export function installLocale(root: DocumentRoot): Locale {
  return applyLocale(root, DEFAULT_LOCALE);
}
