import { formatMinor, JOD } from "@pos/money";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("back office", () => {
  it("renders", () => {
    render(<App />);
    expect(screen.getByRole("heading")).toBeDefined();
  });

  /**
   * The back office is where reports are read, so it is the other consumer of
   * the exponent rule (I-2). Both apps resolve `@pos/money` to one module, and
   * this asserts the workspace wiring actually holds — a duplicated formatter
   * with a different default is precisely the bug the shared package prevents.
   */
  it("formats money through the shared module, at JOD's three digits", () => {
    expect(formatMinor(2500, JOD)).toBe("2.500");
  });
});
