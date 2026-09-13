import { JOD, MoneyError, USD } from "@pos/money";
import { describe, expect, it } from "vitest";
import {
  FormatError,
  formatCatalogMoney,
  formatDate,
  formatMoney,
  formatQty,
} from "./format";

/** Arabic-Indic and extended Arabic-Indic digits — what must never appear. */
const EASTERN_DIGITS = /[٠-٩۰-۹]/;

describe("formatMoney", () => {
  /**
   * I-2. JOD's exponent is 3, so a total is written to the fils. The defect
   * this guards is the one `packages/money` records in its own header: a
   * formatter that defaults to two decimal places renders every dinar amount
   * a factor of ten wrong, and both numbers look plausible on a screen.
   */
  it("formats_jod_at_the_currency_exponent", () => {
    expect(formatMoney(2500, JOD)).toBe("2.500");
    expect(formatMoney(1, JOD)).toBe("0.001");
    expect(formatMoney(0, JOD)).toBe("0.000");
    expect(formatMoney(-2500, JOD)).toBe("-2.500");

    // The exponent is read from the currency, never assumed.
    expect(formatMoney(2500, USD)).toBe("25.00");
  });

  it("groups the integer part and never the fils", () => {
    expect(formatMoney(1234567, JOD)).toBe("1,234.567");
    expect(formatMoney(1000000000, JOD)).toBe("1,000,000.000");
    expect(formatMoney(-1234567, JOD)).toBe("-1,234.567");
  });

  it("refuses an amount a float has touched, rather than rounding it", () => {
    expect(() => formatMoney(1.5, JOD)).toThrow(MoneyError);
  });

  it("carries a bigint amount past Number.MAX_SAFE_INTEGER", () => {
    expect(formatMoney(9007199254740993n, JOD)).toBe("9,007,199,254,740.993");
  });
});

describe("formatCatalogMoney", () => {
  /**
   * "A shorter catalogue display is allowed only when exact." Dropping a
   * trailing zero is exact; dropping a digit is a shelf label that disagrees
   * with the till, which makes the customer right and the merchant wrong at
   * the counter.
   */
  it("catalog_short_format_refuses_to_hide_fils", () => {
    // Exact, so shortening is allowed.
    expect(formatCatalogMoney(2500, JOD)).toBe("2.5");
    expect(formatCatalogMoney(2000, JOD)).toBe("2");

    // Not exact, so the full exponent stands.
    expect(formatCatalogMoney(2505, JOD)).toBe("2.505");
    expect(formatCatalogMoney(2050, JOD)).toBe("2.05");
    expect(formatCatalogMoney(1, JOD)).toBe("0.001");

    // A transaction amount is never shortened, whatever the catalogue shows.
    expect(formatMoney(2500, JOD)).toBe("2.500");
  });
});

describe("digits", () => {
  /**
   * §10 makes Arabic the product rather than a translation, and a Jordanian
   * minimarket still writes prices in 0-9. The risk is not the money package,
   * which emits ASCII by construction — it is `Intl`, which resolves `ar` to
   * the `arab` numbering system and would date a receipt ٢٠٢٦.
   */
  it("uses_western_digits_in_arabic_locale", () => {
    const money = formatMoney(1234567, JOD, "ar");
    const catalog = formatCatalogMoney(2500, JOD, "ar");
    const date = formatDate("2026-09-13T21:30:00Z", "Asia/Amman", "ar");

    for (const rendered of [money, catalog, date]) {
      expect(rendered).not.toMatch(EASTERN_DIGITS);
    }

    // The digit rule does not vary by locale, so neither does the output.
    expect(money).toBe(formatMoney(1234567, JOD, "en"));
    expect(catalog).toBe(formatCatalogMoney(2500, JOD, "en"));
    expect(date).toBe(formatDate("2026-09-13T21:30:00Z", "Asia/Amman", "en"));
  });
});

describe("formatQty", () => {
  it("writes a discrete quantity as a whole number of units", () => {
    expect(formatQty(1000, false)).toBe("1");
    expect(formatQty(3000, false)).toBe("3");
    expect(formatQty(0, false)).toBe("0");
    expect(formatQty(-2000, false)).toBe("-2");
  });

  it("writes a weighed quantity to the gram", () => {
    expect(formatQty(1250, true)).toBe("1.250");
    expect(formatQty(2000, true)).toBe("2.000");
    expect(formatQty(1, true)).toBe("0.001");
  });

  /**
   * A discrete line that is not a whole unit is a defect upstream. Rounding it
   * to a tidy integer would make the wrong number look right, so it is shown.
   */
  it("shows a discrete quantity that is not a whole unit rather than hiding it", () => {
    expect(formatQty(1500, false)).toBe("1.500");
  });

  it("refuses a quantity a float has touched", () => {
    expect(() => formatQty(1.5, false)).toThrow(FormatError);
  });
});

describe("formatDate", () => {
  /**
   * The zone is an argument, so a business date is assertable from any
   * machine. Jordan is permanently UTC+3, so 21:30Z is 00:30 the next day —
   * the boundary that decides which shift a sale belongs to.
   */
  it("resolves an instant in the named zone, not the host's", () => {
    expect(formatDate("2026-09-13T21:30:00Z", "Asia/Amman")).toBe(
      "2026-09-14 00:30",
    );
    expect(formatDate("2026-09-13T21:30:00Z", "UTC")).toBe("2026-09-13 21:30");
  });

  it("keeps one layout across locales, so records collate", () => {
    const instant = "2026-01-05T08:07:00Z";
    expect(formatDate(instant, "Asia/Amman", "ar")).toBe("2026-01-05 11:07");
    expect(formatDate(instant, "Asia/Amman", "en")).toBe("2026-01-05 11:07");
  });

  it("names its refusals instead of falling back to UTC or Invalid Date", () => {
    expect(() => formatDate("not a date", "Asia/Amman")).toThrow(FormatError);
    expect(() => formatDate("2026-09-13T21:30:00Z", "Mars/Olympus")).toThrow(
      FormatError,
    );
  });
});
