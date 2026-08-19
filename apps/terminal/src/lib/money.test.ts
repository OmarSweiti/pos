import { describe, expect, it } from "vitest";
import { formatMinor } from "./money";

describe("formatMinor", () => {
  it("formats 2-digit minor units", () => {
    expect(formatMinor(1000)).toBe("10.00");
    expect(formatMinor(1)).toBe("0.01");
    expect(formatMinor(-2599)).toBe("-25.99");
  });
  it("supports 3-digit currencies (e.g. JOD fils)", () => {
    expect(formatMinor(2500, 3)).toBe("2.500");
  });
});
