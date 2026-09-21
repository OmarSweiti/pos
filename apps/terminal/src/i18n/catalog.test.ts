/**
 * Microstep 1.11.1's proof (gap G-5).
 *
 * `phase-1:1256` makes one command answer three questions — Arabic is the
 * rendered default, the two catalogues carry identical typed keys, and the UI
 * resolves the exact font asset 1.7.2 proved — so all three are asserted here
 * rather than spread across the files they happen to belong to.
 */

import { DEFAULT_LOCALE, directionFor, LOCALES } from "@pos/ui";
import { describe, expect, inject, it } from "vitest";
import type { DocumentRoot } from "../lib/direction";
import { ar } from "./ar";
import { en } from "./en";
import { installLocale, messages, t } from "./index";

/**
 * What `vite.config.ts` read on the suite's behalf. Declared where it is used:
 * the augmentation is the contract between the config and this file, and
 * keeping it here means a change to either side fails to compile rather than
 * arriving as `undefined` at run time.
 */
declare module "vitest" {
  interface ProvidedContext {
    fontResolution: {
      screenCss: string;
      screenCssHref: string;
      rasteriserRust: string;
      rasteriserRustHref: string;
      presentHrefs: string[];
    };
  }
}

/** Conventions §2: `<screen>.<element>.<variant>`, lower-case, dot-separated. */
const KEY_SHAPE = /^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*){2}$/;

/**
 * Any digit, in either script. `[0-9]` alone would be the wrong guard here:
 * conventions §10 mandates Western Arabic digits *because* Eastern
 * Arabic-Indic ones (٠-٩) confuse a Jordanian reader more than they serve, so
 * an Eastern digit baked into a catalogue entry is the worse version of the
 * defect this refuses, not an exemption from it.
 */
const DIGIT = /[0-9\u0660-\u0669\u06F0-\u06F9]/u;

/**
 * Every `url("…")` in a stylesheet, in source order.
 *
 * Deliberately not a CSS parser. The one thing this must not do is accept a
 * stylesheet it failed to understand — an empty result is treated as a failure
 * by every caller below, so a `@font-face` written in a form this misses reads
 * as "the UI names no font" and reds, rather than as agreement.
 */
function cssUrls(css: string): string[] {
  return [...css.matchAll(/url\(\s*["']?([^"')]+)["']?\s*\)/g)].map(
    (match) => match[1],
  );
}

/** Every `include_bytes!("…")` argument in a Rust source file, in source order. */
function includedBytes(rust: string): string[] {
  return [...rust.matchAll(/include_bytes!\(\s*"([^"]+)"\s*\)/g)].map(
    (match) => match[1],
  );
}

/** Resolve each relative reference against the file that wrote it. */
function resolveAll(references: string[], baseHref: string): string[] {
  return references.map((reference) => new URL(reference, baseHref).href);
}

describe("message catalogues", () => {
  /**
   * The named test.
   *
   * `en.ts` is annotated `Catalog<MessageKey>`, so this is already a compile
   * error in both directions today — and that is exactly why it is also
   * asserted here. The type's excess-property half only fires on an object
   * literal; a catalogue later built by spread, by `Object.assign` or by a
   * helper keeps the missing-key check and quietly loses the extra-key one.
   * This assertion survives that refactor, which is the only kind of guard
   * worth writing beside a type.
   */
  it("catalogs_have_identical_key_sets", () => {
    const arabic = Object.keys(ar).sort();
    const english = Object.keys(en).sort();

    expect(arabic).toEqual(english);
    expect(arabic.length).toBeGreaterThan(0);

    // And through the lookup the product actually uses, so a catalogue wired
    // into `messages` under the wrong locale is caught too.
    for (const locale of LOCALES) {
      expect(Object.keys(messages[locale]).sort()).toEqual(arabic);
    }
  });

  it("every_key_follows_the_screen_element_variant_convention", () => {
    for (const key of Object.keys(ar)) {
      expect(key, `${key} is not <screen>.<element>.<variant>`).toMatch(
        KEY_SHAPE,
      );
    }
  });

  /**
   * Two ways a catalogue entry is worse than useless, and neither is a missing
   * key. An empty string renders as a blank button; a digit inside a phrase is
   * a number that bypassed `formatMoney` and its currency exponent, which
   * conventions §10 forbids precisely because the register may not compute
   * money on the screen side.
   */
  it("no_catalogue_entry_is_empty", () => {
    for (const locale of LOCALES) {
      for (const [key, text] of Object.entries(messages[locale])) {
        expect(text.trim(), `${locale}:${key} is blank`).not.toBe("");
        expect(text, `${locale}:${key} embeds a numeral`).not.toMatch(DIGIT);
      }
    }
  });

  it("t_returns_the_catalogue_entry_for_the_active_locale", () => {
    expect(t("ar", "sale.action.pay")).toBe(ar["sale.action.pay"]);
    expect(t("en", "sale.action.pay")).toBe(en["sale.action.pay"]);
    expect(t("ar", "sale.action.pay")).not.toBe(t("en", "sale.action.pay"));
  });
});

describe("the rendered default", () => {
  /**
   * The `Done when`'s first condition, and the deliverable handoff §5 ruling 4
   * reserves for this microstep: *"supplying `dir="rtl"` to jsdom is a
   * test-fixture fact, not that deliverable."*
   *
   * So the root this runs against says the opposite of the answer. The harness
   * hands jsdom the real `index.html`, which already reads `dir="rtl"` — assert
   * on that and the test passes with no product code at all, which is precisely
   * what `Sale.test.tsx` says it cannot yet rule out.
   */
  it("arabic_is_the_rendered_default_on_a_document_that_says_otherwise", () => {
    const root: DocumentRoot = { lang: "en", dir: "ltr" };

    const applied = installLocale(root);

    expect(applied).toBe(DEFAULT_LOCALE);
    expect(root.lang).toBe("ar");
    expect(root.dir).toBe("rtl");
    expect(root.dir).toBe(directionFor(DEFAULT_LOCALE));
  });

  /**
   * And the register's real entry point, on its real document — the assertion
   * that makes the one above more than a unit test of a function nobody calls.
   *
   * `1.2.6` could not prove its own call site: on an FTS5-enabled build, an
   * `open` that checks and one that does not are indistinguishable, so the
   * call is reviewed rather than tested. This call site is not in that class,
   * because its effect is observable — so it is tested rather than reviewed.
   * Importing `main.tsx` boots the register exactly as the browser does,
   * against a document flipped to `ltr`/`en` first so no pass can come from
   * the harness's `index.html` fixture. Deleting the one line from `main.tsx`
   * turns this red; deleting it while keeping `installLocale` turns *only*
   * this red, which is the whole point of having both.
   *
   * Last in the file on purpose: it mounts the application into `#root`, and
   * Testing Library's `cleanup` does not own that tree.
   */
  it("the_entry_point_applies_the_locale_before_the_first_render", async () => {
    const root = document.documentElement;
    root.lang = "en";
    root.dir = "ltr";

    await import("../main");

    expect(root.lang).toBe(DEFAULT_LOCALE);
    expect(root.dir).toBe(directionFor(DEFAULT_LOCALE));
    // Not just the attribute: the direction the rendered tree inherits.
    expect(getComputedStyle(document.body).direction).toBe("rtl");
  });
});

describe("the embedded typeface", () => {
  const fonts = inject("fontResolution");

  /**
   * The named test, and 1.7.2's inherited obligation: the receipt must be drawn
   * with the file the screen is painted from.
   *
   * Both sides name their faces relatively, so each set is resolved against the
   * file that wrote it and the two are compared as absolute URLs. A change on
   * either side — a copy under `public/`, a subset committed beside the
   * original, a rasteriser pointed at a different weight — turns this red,
   * which is the only thing standing between "one font" and a receipt that
   * stops looking like the screen.
   */
  it("ui_and_rasterizer_resolve_the_same_embedded_font", () => {
    const screen = resolveAll(
      cssUrls(fonts.screenCss),
      fonts.screenCssHref,
    ).sort();
    const rasteriser = resolveAll(
      includedBytes(fonts.rasteriserRust),
      fonts.rasteriserRustHref,
    ).sort();

    // `font.rs` also embeds the OFL licence with `include_str!`, which is not
    // an `include_bytes!` and so is not in this set. Both faces are.
    expect(screen.length).toBeGreaterThan(0);
    expect(screen).toEqual(rasteriser);

    // A path can resolve tidily and name nothing. Both sides would still agree.
    for (const href of screen) {
      expect(fonts.presentHrefs, `${href} is not in assets/fonts/`).toContain(
        href,
      );
    }
  });

  /*
   * The one mutation this file cannot catch, recorded rather than left to be
   * rediscovered: point **both** sides at `assets/fonts/LICENSE.txt`. The two
   * sets still agree, the file still exists, and this suite says the UI and
   * the rasteriser resolve the same embedded font — which they do. It is not
   * a font.
   *
   * What refuses it is 1.7.2, in another crate and another runner:
   * `crates/pos-hardware/tests/font_asset.rs` pins both face *names* against
   * the compiled bytes and asserts each opens with the TrueType `sfnt` magic,
   * so a licence in either slot fails there. Measured, not assumed — the
   * mutation was applied and `cargo nextest run -p pos-hardware` failed on
   * `the_committed_faces_match_the_compiled_ones`.
   *
   * So the guarantee composes — paths equal here, bytes proven a real face
   * there — but it composes **across two runners**, and the `Done when`'s
   * single vitest command does not carry it alone. Re-deriving it in
   * TypeScript would be the second hand-maintained copy of a fact, which is
   * the worse of the two failures.
   */

  /**
   * The family name is the other half of "the same font". Two `@font-face`
   * rules can point at the right files and declare a family nothing else in
   * the app asks for, leaving every element on the fallback stack while this
   * suite stays green.
   */
  it("the_font_css_names_the_family_the_rasteriser_reports", () => {
    const declared = [
      ...fonts.rasteriserRust.matchAll(
        /pub const FAMILY:\s*&str\s*=\s*"([^"]+)"/g,
      ),
    ].map((match) => match[1]);

    expect(declared).toHaveLength(1);
    const family = declared[0];

    const faces = [
      ...fonts.screenCss.matchAll(/font-family:\s*"([^"]+)"/g),
    ].map((match) => match[1]);

    expect(faces.length).toBeGreaterThan(0);
    expect(new Set(faces)).toEqual(new Set([family]));

    // Declaring the family is not asking for it. Both halves of the binding
    // are checked because either one alone leaves every element on the
    // fallback stack: the variable must carry the family, and `:root` must
    // actually resolve it.
    expect(fonts.screenCss).toContain(`--font-ui: "${family}"`);
    expect(fonts.screenCss).toMatch(/font-family:\s*var\(--font-ui\)/);
  });
});
