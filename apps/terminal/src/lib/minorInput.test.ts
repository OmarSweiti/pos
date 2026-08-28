import { describe, expect, it } from "vitest";
import { parseCountInput, parseMinorInput } from "./minorInput";

describe("parseMinorInput", () => {
  it("accepts whole minor units", () => {
    expect(parseMinorInput("2500")).toEqual({ ok: true, value: 2500 });
    expect(parseMinorInput("0")).toEqual({ ok: true, value: 0 });
    expect(parseMinorInput("-1")).toEqual({ ok: true, value: -1 });
  });

  /**
   * The regression this module exists for. 1.5 is not 1.5 minor units; it is a
   * float that reached a field whose contract is i64, and the old input wrote it
   * to the store without asking.
   */
  it("refuses a fraction rather than rounding it (I-1)", () => {
    expect(parseMinorInput("1.5")).toEqual({
      ok: false,
      reason: "money must be whole minor units",
    });
    expect(parseMinorInput("0.001").ok).toBe(false);
  });

  it("refuses a magnitude that is no longer an exact i64", () => {
    expect(parseMinorInput(String(Number.MAX_SAFE_INTEGER + 2)).ok).toBe(false);
  });

  it("refuses an empty field instead of reading it as zero", () => {
    expect(parseMinorInput("")).toEqual({ ok: false, reason: "empty" });
    expect(parseMinorInput("   ")).toEqual({ ok: false, reason: "empty" });
  });

  it("refuses what is not a number at all", () => {
    expect(parseMinorInput("abc").ok).toBe(false);
    expect(parseMinorInput("Infinity")).toEqual({
      ok: false,
      reason: "not a number",
    });
  });

  it("gives every refusal a reason, because a silent field reads as broken", () => {
    for (const raw of ["", "abc", "1.5", "Infinity"]) {
      const result = parseMinorInput(raw);
      expect(result.ok).toBe(false);
      if (!result.ok) expect(result.reason.length).toBeGreaterThan(0);
    }
  });
});

describe("parseCountInput", () => {
  it("accepts a whole count at or above the minimum", () => {
    expect(parseCountInput("3")).toEqual({ ok: true, value: 3 });
    expect(parseCountInput("1")).toEqual({ ok: true, value: 1 });
  });

  it("refuses a fractional count", () => {
    expect(parseCountInput("2.5")).toEqual({
      ok: false,
      reason: "must be a whole number",
    });
  });

  it("refuses below the minimum, so a split into zero parts cannot be asked for", () => {
    expect(parseCountInput("0")).toEqual({
      ok: false,
      reason: "must be at least 1",
    });
    expect(parseCountInput("-2").ok).toBe(false);
  });
});
