import { describe, expect, it } from "vitest";
import {
  applyLocale,
  DEFAULT_LOCALE,
  type DocumentRoot,
  directionFor,
  type Locale,
  toggled,
  toggleLabel,
} from "./direction";

const LOCALES: readonly Locale[] = ["ar", "en"];

describe("locale and direction", () => {
  it("defaults to Arabic, right-to-left", () => {
    expect(DEFAULT_LOCALE).toBe("ar");
    expect(directionFor(DEFAULT_LOCALE)).toBe("rtl");
  });

  it("maps each locale to its direction", () => {
    expect(directionFor("ar")).toBe("rtl");
    expect(directionFor("en")).toBe("ltr");
  });

  it("switches to the opposite selection, and back again", () => {
    expect(toggled("ar")).toBe("en");
    expect(toggled("en")).toBe("ar");
    for (const locale of LOCALES) {
      expect(toggled(toggled(locale))).toBe(locale);
    }
  });

  it("flips lang and dir together", () => {
    const root: DocumentRoot = { lang: "ar", dir: "rtl" };
    applyLocale(root, "en");
    expect(root).toEqual({ lang: "en", dir: "ltr" });
    applyLocale(root, "ar");
    expect(root).toEqual({ lang: "ar", dir: "rtl" });
  });

  it("never leaves lang and dir disagreeing, however it is driven", () => {
    const root: DocumentRoot = { lang: "en", dir: "ltr" };
    for (const locale of ["ar", "en", "ar", "ar", "en", "en"] as const) {
      applyLocale(root, locale);
      expect(root.dir).toBe(directionFor(root.lang as Locale));
    }
  });

  it("labels the toggle with the language it switches to", () => {
    expect(toggleLabel("ar")).toBe("English");
    expect(toggleLabel("en")).toBe("العربية");
  });
});
