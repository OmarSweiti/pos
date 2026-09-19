/**
 * Microstep 1.11.6 — global scan capture.
 *
 * The two named tests run on a fake clock, as the microstep requires, and they
 * drive real `keydown` events through `user-event` rather than calling
 * {@link feedScanKey} with hand-written timestamps. That distinction is the
 * whole point of the exercise: the heuristic is easy to satisfy on paper, and
 * the thing `ref/ui-spec.md:140` says implementations get wrong is what happens
 * to the keystrokes on the way past a focused field.
 *
 * `userEvent.setup({ delay })` is what makes the clock move between characters.
 * Measured, because it is the detail the whole file rests on: `delay` is a
 * **setup** option, not an option to `type` or `keyboard`, and passing it to the
 * call instead leaves every keystroke on the same timestamp — which reads as a
 * scanner that never scans. With it in `setup`, consecutive `keydown` events are
 * exactly `delay` milliseconds apart on the fake clock.
 *
 * This file is a `.ts`, as the microstep's `Files:` line names it, so the DOM is
 * built with `document.createElement` rather than JSX — Vite 8 runs on Oxc,
 * which refuses JSX in a `.ts` file. Nothing here needs React: the module under
 * test is a listener and a state machine, and the search box it has to survive
 * is an `<input>` whatever renders it.
 */

import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  attachScanCapture,
  feedScanKey,
  NO_SCAN_CANDIDATE,
  SCAN_MAX_GAP_MS,
  type ScanCandidate,
  type ScanStep,
} from "./scanner";

/** A barcode, and the speed each of the two senders types it at. */
const BARCODE = "629104";
const SCANNER_GAP_MS = 5;
const CASHIER_GAP_MS = 50;

/**
 * Testing Library's `cleanup` only removes containers it rendered, and nothing
 * here renders. Elements appended by hand would otherwise survive into the next
 * test and be found by the next `document.activeElement` assertion.
 */
afterEach(() => {
  document.body.replaceChildren();
  vi.useRealTimers();
});

beforeEach(() => {
  vi.useFakeTimers();
});

/** `userEvent`, bound to the fake clock with a fixed gap between keystrokes. */
function typistAt(gapMs: number) {
  return userEvent.setup({
    delay: gapMs,
    advanceTimers: vi.advanceTimersByTime,
    document,
  });
}

/** A focused search box holding what a cashier had already typed into it. */
function focusedSearchBox(initial: string): HTMLInputElement {
  const search = document.createElement("input");
  search.setAttribute("aria-label", "بحث");
  document.body.append(search);
  search.focus();
  search.value = initial;
  search.setSelectionRange(initial.length, initial.length);
  return search;
}

describe("global scan capture, on a fake clock", () => {
  /**
   * The named test: the same characters, the same terminator, the same
   * listener — and the only difference is how fast they arrive.
   *
   * The cashier's half is not decoration. Without it the test would pass
   * against a module that called every Enter-terminated sequence a scan, which
   * is the failure this heuristic exists to prevent: a cashier searching for
   * خبز and pressing Enter would have their search text swallowed and routed as
   * a barcode.
   */
  it("scan_burst_detected_over_typing", async () => {
    const scanned: string[] = [];
    const detach = attachScanCapture(document, {
      onScan: (code) => scanned.push(code),
    });

    await typistAt(SCANNER_GAP_MS).keyboard(`${BARCODE}{Enter}`);
    expect(scanned).toEqual([BARCODE]);

    await typistAt(CASHIER_GAP_MS).keyboard(`${BARCODE}{Enter}`);
    expect(scanned).toEqual([BARCODE]);

    detach();
  });

  /**
   * The named test the microstep calls the hard one, and it asserts two things
   * because only the pair is "routing correctly".
   *
   * The scan must reach the handler — and the search box must be left as the
   * cashier left it. A capture-phase listener can only recognise a burst once a
   * second character has arrived inside the threshold, so by then the first one
   * has already been inserted into whatever had focus. Suppression from that
   * point leaves exactly one stray character behind; measured against this
   * harness, the field ends up holding `خبز6`. The module takes it back, and
   * the second assertion is the one that would catch its removal.
   */
  it("scan_routes_while_search_focused", async () => {
    const search = focusedSearchBox("خبز");
    const scanned: string[] = [];
    const detach = attachScanCapture(document, {
      onScan: (code) => scanned.push(code),
    });

    await typistAt(SCANNER_GAP_MS).keyboard(`${BARCODE}{Enter}`);

    expect(scanned).toEqual([BARCODE]);
    expect(search.value).toBe("خبز");
    expect(document.activeElement).toBe(search);

    detach();
  });

  /**
   * Suppression, isolated from retraction — and this test exists because its
   * absence was measured, not imagined.
   *
   * `scan_routes_while_search_focused` asserts the field's *final* value, and
   * retraction restores that value from a snapshot taken before the burst
   * began. So it passes whether the absorbed characters were suppressed on the
   * way in or merely undone on the way out: removing `preventDefault` from the
   * absorbed branch leaves every assertion in this file green. The two halves
   * of the guard mask each other, which is the shape of defect #178 shipped and
   * the 15 September audit caught.
   *
   * Undoing is not equivalent to never inserting. A field that receives all six
   * characters fires an `input` event for each one, so a controlled search box
   * would run its query six times and repaint the barcode before it vanished.
   * Watching the events rather than the end state is what separates the two.
   */
  it("never lets an absorbed character reach the field", async () => {
    const search = focusedSearchBox("خبز");
    const observed: string[] = [];
    search.addEventListener("input", () => observed.push(search.value));
    const detach = attachScanCapture(document, { onScan: () => {} });

    await typistAt(SCANNER_GAP_MS).keyboard(`${BARCODE}{Enter}`);

    // Exactly one, and it is the character physics forces through: the burst is
    // not recognisable until a second character has arrived to be timed against
    // the first.
    expect(observed).toEqual(["خبز6"]);

    detach();
  });

  /**
   * The cashier keeps their keystrokes. The mirror of the test above: at typing
   * speed nothing is suppressed and nothing is retracted, so the search box
   * ends up holding what was typed into it.
   */
  it("leaves typed characters in the field it is watching", async () => {
    const search = focusedSearchBox("");
    const scanned: string[] = [];
    const detach = attachScanCapture(document, {
      onScan: (code) => scanned.push(code),
    });

    await typistAt(CASHIER_GAP_MS).keyboard("42");

    expect(search.value).toBe("42");
    expect(scanned).toEqual([]);

    detach();
  });

  /** Detaching stops the listener; a later burst is nobody's business. */
  it("stops capturing once detached", async () => {
    const scanned: string[] = [];
    attachScanCapture(document, { onScan: (code) => scanned.push(code) })();

    await typistAt(SCANNER_GAP_MS).keyboard(`${BARCODE}{Enter}`);

    expect(scanned).toEqual([]);
  });
});

/**
 * The heuristic on its own, fed timestamps directly.
 *
 * These cases are about the boundary and the shapes a scanner or a cashier can
 * produce that the two DOM tests above do not reach. Time is an argument here,
 * so none of them needs a clock at all — which is the reason the state machine
 * is separable from the listener in the first place.
 */
describe("the scan heuristic", () => {
  /**
   * The last verdict of a sequence.
   *
   * Written out rather than using the ES2022 array accessor, because
   * `tsconfig.app.json` targets ES2020 and lists `lib: ["ES2020", "DOM",
   * "DOM.Iterable"]`. `vitest` transpiles without typechecking and runs the
   * ES2022 form happily, so the only thing that catches it is `tsc -b` inside
   * `just build-web` — which is exactly where it was caught.
   */
  function lastStep(steps: readonly ScanStep[]): ScanStep | undefined {
    return steps[steps.length - 1];
  }

  /** Feed a whole sequence of `[key, at]` pairs and collect the verdicts. */
  function feedAll(keys: readonly (readonly [string, number])[]) {
    let candidate: ScanCandidate = NO_SCAN_CANDIDATE;
    const steps = keys.map(([key, at]) => {
      const advanced = feedScanKey(candidate, key, at);
      candidate = advanced.candidate;
      return advanced.step;
    });
    return { steps, candidate };
  }

  /**
   * The plan says "< 30 ms", so 30 itself is typing. A boundary stated in prose
   * is a boundary that gets flipped to `<=` by whoever next touches it.
   */
  it("treats a gap of exactly the threshold as typing", () => {
    const { steps } = feedAll([
      ["6", 0],
      ["2", SCAN_MAX_GAP_MS],
      ["Enter", SCAN_MAX_GAP_MS * 2],
    ]);

    expect(steps.map((step) => step.kind)).toEqual([
      "typing",
      "typing",
      "typing",
    ]);
  });

  it("absorbs a gap one millisecond under the threshold", () => {
    const { steps } = feedAll([
      ["6", 0],
      ["2", SCAN_MAX_GAP_MS - 1],
    ]);

    expect(steps.map((step) => step.kind)).toEqual(["typing", "absorbed"]);
  });

  /**
   * A scanner reading an upper-case barcode sends `Shift` between characters.
   * A state machine that reset on every key it did not recognise would score
   * every such scan as typing — so unrecognised keys must pass through the
   * candidate untouched rather than clear it.
   */
  it("keeps the burst across interleaved modifier keys", () => {
    const { steps } = feedAll([
      ["A", 0],
      ["Shift", 2],
      ["B", 4],
      ["Enter", 6],
    ]);

    expect(steps.map((step) => step.kind)).toEqual([
      "typing",
      "ignored",
      "absorbed",
      "scanned",
    ]);
    expect(lastStep(steps)).toEqual({ kind: "scanned", code: "AB", leaked: 1 });
  });

  /**
   * Exactly one character reaches the field before a burst can be recognised —
   * never two, and never none. The listener retracts `leaked` characters, so if
   * this number were ever wrong the search box would keep a stray digit or lose
   * a real one.
   */
  it("reports exactly one leaked character for any burst length", () => {
    for (const length of [2, 3, 8, 13]) {
      const keys = Array.from(
        { length },
        (_, index) => [String(index % 10), index * 2] as const,
      );
      const { steps } = feedAll([...keys, ["Enter", length * 2]]);

      expect(lastStep(steps)).toMatchObject({ kind: "scanned", leaked: 1 });
    }
  });

  /** One character and a terminator has no interval, so it cannot be a burst. */
  it("does not call a single character a scan", () => {
    const { steps } = feedAll([
      ["6", 0],
      ["Enter", 2],
    ]);

    expect(steps.map((step) => step.kind)).toEqual(["typing", "typing"]);
  });

  /** Enter with nothing behind it belongs to whatever the cashier is confirming. */
  it("passes a bare Enter through", () => {
    const { steps } = feedAll([["Enter", 0]]);

    expect(steps).toEqual([{ kind: "typing" }]);
  });

  /** A burst nobody terminated stays uncommitted — Enter is what commits it. */
  it("commits nothing until Enter arrives", () => {
    const { steps, candidate } = feedAll([
      ["6", 0],
      ["2", 2],
      ["9", 4],
    ]);

    expect(steps.some((step) => step.kind === "scanned")).toBe(false);
    expect(candidate.chars).toEqual(["6", "2", "9"]);
  });

  /** Two scans in a row: the second starts clean rather than trailing the first. */
  it("starts a fresh candidate after a committed scan", () => {
    const { steps } = feedAll([
      ["6", 0],
      ["2", 2],
      ["Enter", 4],
      ["7", 6],
      ["3", 8],
      ["Enter", 10],
    ]);

    expect(steps[2]).toEqual({ kind: "scanned", code: "62", leaked: 1 });
    expect(lastStep(steps)).toEqual({ kind: "scanned", code: "73", leaked: 1 });
  });

  /** A slow keystroke mid-burst abandons what came before it. */
  it("discards a burst interrupted by a slow keystroke", () => {
    const { steps } = feedAll([
      ["6", 0],
      ["2", 2],
      ["9", 2 + SCAN_MAX_GAP_MS],
      ["1", 4 + SCAN_MAX_GAP_MS],
      ["Enter", 6 + SCAN_MAX_GAP_MS],
    ]);

    expect(lastStep(steps)).toEqual({ kind: "scanned", code: "91", leaked: 1 });
  });
});
