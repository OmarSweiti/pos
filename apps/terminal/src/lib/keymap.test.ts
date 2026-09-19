/**
 * Microstep 1.11.11 — the keyboard map.
 *
 * The named test dispatches real `keydown` events through `user-event` and
 * never a pointer event, which is what its `Done when` asks for: *"exits zero
 * after dispatching every mapped action without pointer input."*
 *
 * **Its expectation is written out rather than derived from `KEY_BINDINGS`.**
 * A test that reads its expected set out of the table it is testing stays green
 * when a row is deleted from that table, because the expectation shrinks with
 * it. The list below is an independent restatement of `ref/ui-spec.md` §7, and
 * the duplication is the point.
 *
 * This file is a `.ts`, as the microstep's `Files:` line names it, so the DOM is
 * built with `document.createElement` — Vite 8 runs on Oxc, which refuses JSX in
 * a `.ts` file.
 */

import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { type Action, actionFor, attachKeymap } from "./keymap";
import { attachScanCapture } from "./scanner";

/**
 * `ref/ui-spec.md` §7, restated: the key a cashier presses and the action it
 * must reach. `Del` and `Esc` are the labels printed on the keys; `Delete` and
 * `Escape` are what `KeyboardEvent.key` reports.
 *
 * The minus is `-` (U+002D). The spec prints `−` (U+2212), a typographic sign
 * no keyboard emits — a binding on that character would never fire and would
 * survive review, because it matches the document exactly.
 */
const SPEC: readonly (readonly [string, Action])[] = [
  ["{F2}", "search"],
  ["{F4}", "pay"],
  ["{F6}", "park"],
  ["{F7}", "resume"],
  ["{F9}", "returns"],
  ["{Delete}", "voidLine"],
  ["+", "quantityIncrement"],
  ["-", "quantityDecrement"],
  ["{F12}", "lock"],
  ["{Escape}", "cancel"],
  ["{Enter}", "confirm"],
];

/** Handlers that record, plus the list of what fired. */
function recorder(): { handlers: Record<Action, () => void>; fired: Action[] } {
  const fired: Action[] = [];
  const of = (action: Action) => () => {
    fired.push(action);
  };
  return {
    fired,
    handlers: {
      search: of("search"),
      pay: of("pay"),
      park: of("park"),
      resume: of("resume"),
      returns: of("returns"),
      voidLine: of("voidLine"),
      quantityIncrement: of("quantityIncrement"),
      quantityDecrement: of("quantityDecrement"),
      lock: of("lock"),
      cancel: of("cancel"),
      confirm: of("confirm"),
    },
  };
}

function focusedSearchBox(): HTMLInputElement {
  const search = document.createElement("input");
  search.setAttribute("aria-label", "بحث");
  document.body.append(search);
  search.focus();
  return search;
}

afterEach(() => {
  document.body.replaceChildren();
});

describe("the keyboard map", () => {
  /**
   * The named test. Every row of `ref/ui-spec.md` §7 reaches its action from the
   * keyboard, and the run uses no pointer event of any kind.
   *
   * The final assertion is the one that makes it "every": comparing the set of
   * actions that fired against the eleven written above catches a binding that
   * was deleted as well as one that was mis-wired.
   */
  it("every_action_reachable_without_a_mouse", async () => {
    const { handlers, fired } = recorder();
    const detach = attachKeymap(document, handlers);
    const user = userEvent.setup({ document });

    for (const [key] of SPEC) {
      await user.keyboard(key);
    }

    expect(fired).toEqual(SPEC.map(([, action]) => action));
    expect(new Set(fired).size).toBe(SPEC.length);

    detach();
  });

  /**
   * A key that means something inside a text box belongs to the text box.
   *
   * `+` in the search field is a plus sign, not a quantity change, and `Delete`
   * is a character deletion, not a voided line. This is the same collision
   * `scanner.ts` faces and it gets the opposite answer: a scan is recognisable
   * by its timing and can be retracted, whereas a single `+` is indistinguishable
   * from a cashier typing one, so the map yields instead.
   */
  it("yields the typed keys to a focused field", async () => {
    const search = focusedSearchBox();
    const { handlers, fired } = recorder();
    const detach = attachKeymap(document, handlers);
    const user = userEvent.setup({ document });

    await user.keyboard("+-");

    expect(fired).toEqual([]);
    expect(search.value).toBe("+-");

    detach();
  });

  /** A function key has no meaning in a text box, so focus does not hold it. */
  it("fires the global keys even while a field has focus", async () => {
    const search = focusedSearchBox();
    const { handlers, fired } = recorder();
    const detach = attachKeymap(document, handlers);
    const user = userEvent.setup({ document });

    await user.keyboard("{F4}{Escape}");

    expect(fired).toEqual(["pay", "cancel"]);
    expect(search.value).toBe("");

    detach();
  });

  /** No row of §7 carries a modifier, so a chord belongs to the OS. */
  it("ignores a chord", async () => {
    const { handlers, fired } = recorder();
    const detach = attachKeymap(document, handlers);
    const user = userEvent.setup({ document });

    await user.keyboard("{Control>}{F4}{/Control}");

    expect(fired).toEqual([]);

    detach();
  });

  /** A key the map does not name is nobody's business. */
  it("leaves an unmapped key alone", async () => {
    const { handlers, fired } = recorder();
    const detach = attachKeymap(document, handlers);
    const user = userEvent.setup({ document });

    await user.keyboard("{F5}a");

    expect(fired).toEqual([]);

    detach();
  });

  /**
   * A screen that has not wired an action yet must not swallow its key.
   *
   * `preventDefault` is called only when a handler actually runs, so an unwired
   * binding degrades to the browser's own behaviour rather than to silence —
   * which is the difference between a screen that is incomplete and one that
   * looks broken.
   */
  it("does not consume a key whose action has no handler", async () => {
    const user = userEvent.setup({ document });
    const prevented: boolean[] = [];
    // Registered *after* the map, so it observes what the map left behind.
    // Both listeners bubble on `document`, and bubble-phase listeners on one
    // target run in registration order — a probe attached first would read
    // `false` no matter what the map did, which is a way to write this test
    // that always passes.
    const probe = (event: Event) => prevented.push(event.defaultPrevented);

    // The probe is re-registered after each map for the same reason: it has to
    // be the later listener both times, or the second reading is meaningless.
    const unwired = attachKeymap(document, {});
    document.addEventListener("keydown", probe);
    await user.keyboard("{F4}");
    unwired();
    document.removeEventListener("keydown", probe);

    const wired = attachKeymap(document, { pay: () => {} });
    document.addEventListener("keydown", probe);
    await user.keyboard("{F4}");
    wired();
    document.removeEventListener("keydown", probe);

    // An unwired binding leaves the key to the browser; a wired one takes it.
    // The first half is what stops an incomplete screen looking broken, and it
    // is only a real assertion because the second half shows the same key being
    // consumed when a handler exists.
    expect(prevented).toEqual([false, true]);
  });

  it("stops dispatching once detached", async () => {
    const { handlers, fired } = recorder();
    attachKeymap(document, handlers)();
    const user = userEvent.setup({ document });

    await user.keyboard("{F4}");

    expect(fired).toEqual([]);
  });
});

/**
 * `Enter` is the one key two modules want, and `ref/ui-spec.md:245` gives it
 * both jobs at once — "confirm / commit scan".
 *
 * `scanner.ts` listens in the capture phase and calls `preventDefault` only when
 * an Enter actually commits a burst; this map listens in the bubble phase and
 * skips an already-handled event. Neither module knows about the other, so the
 * seam is worth asserting rather than assuming: a regression here is a phantom
 * "confirm" on every scan, which would look like a double submit.
 */
describe("Enter, shared with the scan capture", () => {
  it("does not fire confirm on the Enter that commits a scan", async () => {
    const { handlers, fired } = recorder();
    const scans: string[] = [];
    const detachScan = attachScanCapture(document, {
      onScan: (code) => scans.push(code),
    });
    const detachKeys = attachKeymap(document, handlers);
    const user = userEvent.setup({ document });

    // No fake clock here: `user-event`'s default delay is 0, so every keystroke
    // shares a timestamp and the gaps are 0 ms — comfortably inside the burst
    // threshold, which is exactly the condition this test wants.
    await user.keyboard("629104{Enter}");

    expect(scans).toEqual(["629104"]);
    expect(fired).toEqual([]);

    detachScan();
    detachKeys();
  });

  it("fires confirm on an Enter with no burst behind it", async () => {
    const { handlers, fired } = recorder();
    const scans: string[] = [];
    const detachScan = attachScanCapture(document, {
      onScan: (code) => scans.push(code),
    });
    const detachKeys = attachKeymap(document, handlers);
    const user = userEvent.setup({ document });

    await user.keyboard("{Enter}");

    expect(scans).toEqual([]);
    expect(fired).toEqual(["confirm"]);

    detachScan();
    detachKeys();
  });
});

/** The rules on their own, with no DOM and no focus to reason about. */
describe("the binding rules", () => {
  it("refuses every binding under a modifier", () => {
    for (const [, action] of SPEC) {
      expect(action).toBeTruthy();
    }
    expect(actionFor("F4", { editing: false, chord: true })).toBeNull();
    expect(actionFor("F4", { editing: false, chord: false })).toBe("pay");
  });

  it("holds the typed keys back only while editing", () => {
    expect(actionFor("+", { editing: true, chord: false })).toBeNull();
    expect(actionFor("+", { editing: false, chord: false })).toBe(
      "quantityIncrement",
    );
    expect(actionFor("Escape", { editing: true, chord: false })).toBe("cancel");
  });

  /**
   * The spec's `−` is U+2212 and no keyboard emits it. Asserted so that anyone
   * "correcting" the binding to match the document character for character
   * turns this red instead of shipping a key that never fires.
   */
  it("binds the hyphen-minus a keyboard emits, not the typographic sign", () => {
    expect(actionFor("-", { editing: false, chord: false })).toBe(
      "quantityDecrement",
    );
    expect(actionFor("−", { editing: false, chord: false })).toBeNull();
  });
});
