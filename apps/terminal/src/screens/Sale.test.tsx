/**
 * The canary for the register's DOM component-test harness (microstep 1.11.0).
 *
 * `Sale.tsx` itself is 1.11.5's deliverable and does not exist yet, so this
 * file renders an inline fixture. It deliberately does not render `App`:
 * `App.tsx` calls `invoke` from `@tauri-apps/api/core`, which needs an IPC mock
 * this step does not scope, and it only *prints* `dir=…` as text — it never
 * touches the document root, so an assertion on that text would pass on a
 * left-to-right document.
 *
 * Two of the three tests below are the harness proving itself rather than
 * coverage of a screen. They are here because each one, absent, fails later as
 * something that reads like a bad selector or a flaky timer.
 */

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_LOCALE, directionFor } from "../lib/direction";
import { renderWithProviders } from "../test/setup";

/**
 * A stand-in for the sale screen: enough structure to hold a heading and a
 * region, and no interactive element, so the fixture is not held to a11y rules
 * that belong to the real screen.
 */
function SaleFixture() {
  return (
    <main aria-label="بيع">
      <h1>بيع</h1>
      <output>0.000</output>
    </main>
  );
}

describe("sale screen", () => {
  /**
   * The named test. Both assertions are anchored in product code rather than in
   * the fixture: `directionFor(DEFAULT_LOCALE)` is conventions §10's rule, and
   * the document the harness renders into is `index.html` itself, read by
   * `vite.config.ts`. So flipping `index.html` to `dir="ltr"`, or flipping
   * `DEFAULT_LOCALE` to `en`, turns this red.
   *
   * What it does not yet prove: a subtree that sets no `dir` of its own
   * inherits the root's, so this catches an explicit `dir="ltr"` override and
   * not "no product code ever establishes direction". Applying the locale at
   * boot is 1.11.1's deliverable, and rendering the real screen is 1.11.5's.
   */
  it("sale_screen_renders_in_rtl_by_default", () => {
    const expected = directionFor(DEFAULT_LOCALE);

    renderWithProviders(<SaleFixture />);

    expect(document.documentElement.lang).toBe(DEFAULT_LOCALE);
    expect(document.documentElement.dir).toBe(expected);
    expect(getComputedStyle(screen.getByRole("main")).direction).toBe(expected);
    expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();
  });

  /**
   * The harness's own regression test.
   *
   * `@testing-library/react` registers its automatic `cleanup` only when a
   * global `afterEach` exists, and vitest's `globals` is off here. Without the
   * explicit `afterEach(cleanup)` in `src/test/setup.ts`, this second render
   * accumulates alongside the first and `getAllByRole("main")` returns two —
   * which surfaces in whoever writes 1.11.4 as "Found multiple elements with
   * the role", a message that reads as a bad selector rather than a leaking
   * harness. A one-test canary cannot expose it, so the second test is the
   * proof.
   */
  it("the harness leaves one document behind per test", () => {
    renderWithProviders(<SaleFixture />);

    expect(screen.getAllByRole("main")).toHaveLength(1);
  });
});

describe("sale screen, on a fake clock", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /**
   * The other harness self-proof, and the reason `src/test/setup.ts` defines a
   * minimal `jest` global.
   *
   * Testing Library decides whether to advance a fake clock inside its async
   * wrapper by testing for that global, which vitest does not define. Without
   * the bridge every `await user.*()` under `vi.useFakeTimers()` waits on a
   * `setTimeout(…, 0)` the frozen clock never fires, and the test dies at the
   * timeout rather than at an assertion.
   *
   * 1.11.6's scan-burst tests and 1.11.8's auto-return test both rest on this,
   * so it is proven here rather than discovered there.
   */
  it("advances Testing Library's async wrapper under fake timers", async () => {
    vi.useFakeTimers();
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });

    render(<Typeahead />);
    await user.type(screen.getByRole("textbox"), "٤٢");

    expect(screen.getByRole("textbox")).toHaveValue("٤٢");
  });
});

/** A controlled input, so `user.type` has state to move. */
function Typeahead() {
  const [value, setValue] = useState("");
  return (
    <input
      aria-label="بحث"
      onChange={(event) => setValue(event.target.value)}
      value={value}
    />
  );
}
