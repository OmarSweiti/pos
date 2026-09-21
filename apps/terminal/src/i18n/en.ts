/**
 * The English catalogue (microstep 1.11.1, gap G-5).
 *
 * Annotated `Catalog<MessageKey>` rather than `as const`: that annotation is
 * what makes a key present in `ar.ts` and missing here a compile error, and —
 * because this is an object literal — a key present here and missing there one
 * too. `catalogs_have_identical_key_sets` asserts the same thing at run time,
 * for the reason `packages/ui/src/i18n.ts` records.
 *
 * English is the toggle, not the product. Nothing here is the source of a key.
 */

import type { Catalog } from "@pos/ui";
import type { MessageKey } from "./ar";

export const en: Catalog<MessageKey> = {
  "sale.action.park": "Park",
  "sale.action.resume": "Resume",
  "sale.action.customer": "Customer",
  "sale.action.returns": "Returns",
  "sale.action.pay": "Pay",
  "sale.totals.subtotal": "Subtotal",
  "sale.totals.discount": "Discount",
  "sale.totals.tax": "Tax",
  "sale.totals.total": "Total",
  "sale.search.placeholder": "Type or scan…",
};
