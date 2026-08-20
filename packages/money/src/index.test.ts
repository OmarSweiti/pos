import { describe, expect, it } from "vitest";
import { EUR, formatMinor, JOD, MoneyError, toMinor, USD } from "./index";

describe("formatMinor", () => {
  it("formats JOD at three decimal places, not two (I-2)", () => {
    expect(formatMinor(2500, JOD)).toBe("2.500");
    expect(formatMinor(1, JOD)).toBe("0.001");
    expect(formatMinor(0, JOD)).toBe("0.000");
  });

  it("formats two-decimal currencies at two", () => {
    expect(formatMinor(2500, USD)).toBe("25.00");
    expect(formatMinor(-2599, EUR)).toBe("-25.99");
  });

  it("keeps the sign outside the magnitude", () => {
    expect(formatMinor(-1, JOD)).toBe("-0.001");
    expect(formatMinor(-1000, JOD)).toBe("-1.000");
  });

  it("handles a zero-exponent currency without a stray decimal point", () => {
    expect(formatMinor(1234, { code: "JPY", exponent: 0 })).toBe("1234");
  });

  /**
   * The regression this module exists for: the same integer rendered against
   * the wrong exponent is off by a factor of ten per missing digit. There is no
   * default exponent precisely so this cannot happen by omission.
   */
  it("renders the same amount differently per currency", () => {
    expect(formatMinor(2500, JOD)).toBe("2.500");
    expect(formatMinor(2500, USD)).toBe("25.00");
  });

  it("is exact past Number.MAX_SAFE_INTEGER when given a bigint (I-1)", () => {
    // 2^53 + 1 is not representable as a double; a float path renders …92.
    expect(formatMinor(9007199254740993n, JOD)).toBe("9007199254740.993");
  });

  it("refuses an implausible exponent rather than guessing", () => {
    expect(() => formatMinor(1, { code: "??", exponent: -1 })).toThrow(
      MoneyError,
    );
    expect(() => formatMinor(1, { code: "??", exponent: 9 })).toThrow(
      MoneyError,
    );
  });
});

describe("toMinor", () => {
  it("refuses a fractional amount — a float touched it upstream (I-1)", () => {
    expect(() => toMinor(12.5)).toThrow(MoneyError);
    expect(() => toMinor(0.1 + 0.2)).toThrow(MoneyError);
  });

  it("refuses an amount that is no longer an exact i64", () => {
    expect(() => toMinor(Number.MAX_SAFE_INTEGER + 2)).toThrow(MoneyError);
  });

  it("passes whole minor units through", () => {
    expect(toMinor(2500)).toBe(2500n);
    expect(toMinor(-1)).toBe(-1n);
    expect(toMinor(7n)).toBe(7n);
  });
});
