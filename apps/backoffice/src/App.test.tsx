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
   * The cleanup regression test.
   *
   * `@testing-library/react` registers its automatic `cleanup` only when a
   * global `afterEach` exists, and vitest's `globals` is off across this
   * repository. Without the explicit `afterEach(cleanup)` in
   * `src/test/setup.ts`, this second render accumulates alongside the first
   * and `getAllByRole("main")` returns two — which surfaces in whoever writes
   * the next back-office screen test as "Found multiple elements with the
   * role", a message that reads as a bad selector rather than a leaking
   * harness. A one-render file cannot expose it, so the second render is the
   * proof.
   */
  it("leaves one document behind per test", () => {
    render(<App />);

    expect(screen.getAllByRole("main")).toHaveLength(1);
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
