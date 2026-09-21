/**
 * The Arabic catalogue — the canonical one (microstep 1.11.1, gap G-5).
 *
 * Arabic is the product, so this file defines the key set and `en.ts` is held
 * to it. Reverse the two and a key added in English but forgotten in Arabic
 * would typecheck, which is precisely the direction that must not compile.
 *
 * Keys follow conventions §2 — `<screen>.<element>.<variant>` — and the
 * entries below are the Sale screen's copy as [`ref/ui-spec.md`](../../../../docs/implementation/ref/ui-spec.md)
 * §3 specifies it: the action bar, the totals block and the search field. They
 * are seeded from a normative diagram rather than invented, so `1.11.5` renders
 * these keys instead of replacing them.
 *
 * **No entry contains a digit.** Conventions §10 puts every number on screen
 * through `formatMoney`/`formatQty`/`formatDate` (microstep 1.11.3), so a
 * numeral baked into a translated string is a number that escaped the currency
 * exponent. `every_key_follows_the_screen_element_variant_convention` and its
 * sibling hold both halves.
 */

export const ar = {
  "sale.action.park": "تعليق",
  "sale.action.resume": "استئناف",
  "sale.action.customer": "العميل",
  "sale.action.returns": "المرتجعات",
  "sale.action.pay": "الدفع",
  "sale.totals.subtotal": "المجموع الفرعي",
  "sale.totals.discount": "الخصم",
  "sale.totals.tax": "الضريبة",
  "sale.totals.total": "الإجمالي",
  "sale.search.placeholder": "اكتب أو امسح…",
} as const;

/**
 * Every key the product may render, derived from the canonical catalogue.
 *
 * Declared here rather than in `index.ts` so the dependency runs one way:
 * `en.ts` and `index.ts` both read the key set from the file that defines it,
 * and no module imports the barrel it is exported through.
 */
export type MessageKey = keyof typeof ar;
