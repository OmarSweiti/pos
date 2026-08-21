---
paths: ["apps/terminal/src/**", "apps/backoffice/src/**"]
---

# The front end is Arabic first, and money is still `i64`

Normative: `docs/implementation/01-conventions.md` §10 (internationalisation) and §11 (time).
The shared money rule lives in `packages/money`.

## Money and quantities do not become numbers here (I-1, I-2, I-3)

- **No float touches money in TypeScript either.** Amounts are minor units — `bigint`, or a
  `number` that holds minor units. Never `toFixed`, never `/ 100`, never `parseFloat` on a price,
  never arithmetic on a formatted string.
- **Render and parse through `@pos/money`**: `formatMinor(amount, currency)` and
  `toMinor(amount)`. Nothing else formats an amount, so display precision stays one decision.
- **The exponent is per-currency data (I-2).** `JOD.exponent` is **3** — one dinar is 1000 fils.
  A literal `100` in a money path is a bug, not a shortcut.
- **Quantities are milli-units (I-3).** `1 unit = 1000`. Weighed and discrete share one
  representation, so a "2" from an input is `2000`.
- **A past sale renders from the captured line, never today's catalog (I-5).** Price and name were
  copied onto the line at capture time. Do not re-fetch a product to display or refund a sale.

## Direction is not a theme (§10)

Arabic is not a translation of this product — it is the product, and English is the toggle.

- **CSS logical properties only.** `margin-inline-start`, not `margin-left`; `inset-inline-end`,
  not `right`. Tailwind: `ps-*` `pe-*` `ms-*` `me-*` `start-*` `end-*` `text-start` `text-end`
  `border-s-*` `border-e-*`. **Enforced** — `./scripts/check-logical-css.sh`, in `just lint`.
  A physical side lays out correctly in English and backwards in Arabic, which is the default
  direction, so reviewing the English build will never catch it.
  A genuinely physical case (a raster coordinate, a hardware offset) is allowed with
  `physical-ok: <reason>` on the line.
- **`lang` and `dir` move together, through one function.** `applyLocale` in
  `apps/terminal/src/lib/direction.ts` is the only thing that touches them. A root where they
  disagree renders Arabic text left-to-right.
- **Western Arabic digits (0–9) everywhere.** Eastern Arabic-Indic numerals are not Jordanian
  retail practice.
- **Direction comes from the document root, not from a prop.** `apps/terminal/src/store/locale.ts`
  writes it once; components read it from CSS. `index.html` ships the same default so there is no
  flash of the wrong direction on boot.

## Strings

- **No user-facing string literal in a component.** §10 requires a typed catalog with `ar` and
  `en` kept in lockstep by a test that fails when a key exists in one and not the other.
- **This one is not enforced yet, because the catalog does not exist.** Nothing checks it and
  nothing can until the catalog lands. Until then: do not add user-facing literals, and treat a
  component that needs one as the signal that the catalog is now the blocking work — not as
  permission to hardcode. `toggleLabel` in `direction.ts` is the shape to follow: the label for a
  language lives in that language.

## The boundary

- **Hiding a button is UX, not a permission check.** The check is in Rust, in the command handler
  (`.claude/rules/security.md`). A disabled control is a courtesy to the cashier, never a control.
- **Never log or render a customer's name, phone or email into a console, a toast, or an error
  string.** Same never-list as the backend.
- **Types that cross the IPC boundary are generated, not hand-written.** A hand-copied interface
  is a contract that drifts silently.

## Accessibility is a cashier requirement, not a checkbox

- **Keyboard-only must work end to end** (`02-development-workflow.md` §5.4). A cashier's hands
  are on a keyboard and a scanner, not a mouse. Every action reachable, focus always visible,
  tab order following the sale.
- **Touch targets stay large enough for a fast hand**, and nothing depends on hover, which a
  touchscreen register does not have.
