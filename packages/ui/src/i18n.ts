/**
 * The product's internationalisation contract, shared by both front ends.
 *
 * Conventions §10: *"Arabic is not a translation of this product. It is the
 * product; English is the toggle."* That is product law rather than terminal
 * law — the back office renders the same two languages — so the names it turns
 * on live here and each has exactly one definition. A second copy in an app is
 * how `ar` becomes the default in one place and `en` in another.
 *
 * Nothing here touches a document, a store or a React tree. Applying a locale
 * to a root is the consuming app's job, which is what keeps this file usable
 * from a test, a renderer or a Node script without a DOM.
 */

/** The two languages this product ships in. */
export type Locale = "ar" | "en";

/** Writing direction, as the `dir` attribute spells it. */
export type Direction = "rtl" | "ltr";

/** §10: the register is right-to-left unless someone asks otherwise. */
export const DEFAULT_LOCALE: Locale = "ar";

/** Every locale, in no significant order — for exhaustive iteration. */
export const LOCALES: readonly Locale[] = ["ar", "en"];

export function directionFor(locale: Locale): Direction {
  return locale === "ar" ? "rtl" : "ltr";
}

/**
 * A message catalogue: every key in `K`, each mapped to one string.
 *
 * The type is what holds the catalogues in lockstep at compile time. Declaring
 * the second catalogue as `Catalog<MessageKey>` makes a **missing** key a type
 * error always, and an **extra** key a type error wherever the catalogue is an
 * object literal — which is every catalogue written by hand.
 *
 * It is deliberately not the whole guarantee. A catalogue assembled by spread,
 * by `Object.assign` or by a helper loses the excess-property check, and a
 * widened `string` index would lose both. So the runtime parity test
 * (`catalogs_have_identical_key_sets`) is not redundant with this type: it is
 * the half that survives a refactor of how a catalogue is built.
 */
export type Catalog<K extends string> = Readonly<Record<K, string>>;
