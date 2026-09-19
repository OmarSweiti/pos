/**
 * Global scan capture (microstep 1.11.6).
 *
 * A barcode scanner is a keyboard. `ref/ui-spec.md:16` puts it plainly —
 * "scanning *is* typing — the two are the same input path" — so nothing about
 * the event stream says which one produced it. The only signal is rhythm: a
 * scanner emits characters faster than fingers can, and terminates with Enter.
 *
 * Two things this module owes, and the second is the hard one.
 *
 * **The heuristic.** Characters less than {@link SCAN_MAX_GAP_MS} apart belong
 * to one burst; Enter commits it. That part is arithmetic over timestamps and
 * is pure — {@link feedScanKey} takes the time as an argument, the same
 * discipline `direction.ts` applies to the document root and `pos-domain`
 * applies to clocks, and it is what makes the boundary testable without
 * guessing at a scheduler.
 *
 * **The routing.** `ref/ui-spec.md:140` requires scans to "route correctly even
 * when focus is in the search box", and calls that the detail where most
 * implementations break. It breaks for a reason that no amount of care in the
 * heuristic removes: a burst cannot be *recognised* until a second character
 * arrives inside the threshold, and by then the first character has already
 * been delivered to whatever had focus. Measured against this repository's own
 * harness, a capture-phase listener that suppresses from the second key onward
 * leaves exactly one character sitting in the search box.
 *
 * So suppression alone is not enough, and {@link ScanStep} carries the leak
 * count rather than hiding it: when the scan commits, the characters that got
 * through are taken back out. {@link attachScanCapture} does that by restoring
 * the value the field had before the leaked keystroke, snapshotted in the same
 * capture-phase handler that let it through.
 *
 * **What is deliberately not here.** `ref/ui-spec.md:324` names the component
 * `ScanCapture — the hidden input + timing heuristic`. The hidden input is JSX
 * and belongs to the sale screen (1.11.5); this microstep's `Files:` line names
 * a `src/lib` module, not a component. The input exists so that keystrokes have
 * somewhere to land when nothing else is focused — which is why everything
 * below has to work whether or not anything has focus, and why the capture
 * listens on a root rather than on one element.
 */

/**
 * The inter-character gap that separates a scan from typing.
 *
 * The plan says "< 30 ms between characters", and the comparison below is
 * strictly `<`, so a gap of exactly 30 ms is typing. Stated because it is the
 * kind of boundary that gets silently flipped: 30 ms is about 400 words per
 * minute sustained, which no cashier reaches and every scanner beats.
 */
export const SCAN_MAX_GAP_MS = 30;

/** A burst needs two characters to have an interval between them at all. */
export const SCAN_MIN_LENGTH = 2;

/**
 * What the capture layer should do with the keystroke it just saw.
 *
 * `leaked` appears only on `scanned`, and is the number of characters that
 * reached the focused element before the burst was recognised. By construction
 * it is 1 whenever a burst commits — the character that opened it — and the
 * field exists so the caller cannot forget to take it back.
 */
export type ScanStep =
  | { readonly kind: "ignored" }
  | { readonly kind: "typing" }
  | { readonly kind: "absorbed" }
  | {
      readonly kind: "scanned";
      readonly code: string;
      readonly leaked: number;
    };

/**
 * The characters seen so far and when the last one arrived.
 *
 * `passed` counts the characters this candidate let through to the focused
 * element. It is not derivable from `chars.length`: every character after the
 * first is suppressed, so the two diverge as soon as a burst is recognised.
 */
export interface ScanCandidate {
  readonly chars: readonly string[];
  readonly lastAt: number | null;
  readonly passed: number;
}

export const NO_SCAN_CANDIDATE: ScanCandidate = {
  chars: [],
  lastAt: null,
  passed: 0,
};

/** A single printable character, which is what `KeyboardEvent.key` gives us. */
function isPrintable(key: string): boolean {
  return Array.from(key).length === 1;
}

/**
 * Advance the candidate by one keystroke.
 *
 * Non-printable keys are `ignored` **without resetting the candidate**, and
 * that is load-bearing rather than tidy: a scanner reading an upper-case or
 * symbol-bearing barcode emits `Shift` keydowns interleaved with the
 * characters, and a state machine that reset on every unrecognised key would
 * classify every such scan as typing.
 */
export function feedScanKey(
  candidate: ScanCandidate,
  key: string,
  at: number,
): { readonly candidate: ScanCandidate; readonly step: ScanStep } {
  if (key === "Enter") {
    const code = candidate.chars.join("");
    if (candidate.chars.length >= SCAN_MIN_LENGTH) {
      return {
        candidate: NO_SCAN_CANDIDATE,
        step: { kind: "scanned", code, leaked: candidate.passed },
      };
    }
    // Enter with nothing behind it is the cashier confirming something. It is
    // not ours to swallow — `ref/ui-spec.md:245` maps Enter to "confirm /
    // commit scan", and with no scan to commit only the first half applies.
    return { candidate: NO_SCAN_CANDIDATE, step: { kind: "typing" } };
  }

  if (!isPrintable(key)) {
    return { candidate, step: { kind: "ignored" } };
  }

  const gap =
    candidate.lastAt === null
      ? Number.POSITIVE_INFINITY
      : at - candidate.lastAt;

  if (gap < SCAN_MAX_GAP_MS) {
    return {
      candidate: {
        chars: [...candidate.chars, key],
        lastAt: at,
        passed: candidate.passed,
      },
      step: { kind: "absorbed" },
    };
  }

  // Too slow to belong to the candidate that came before, so it opens a new
  // one — and goes through to whatever has focus, because at one character
  // there is no interval yet and nothing to judge it by. This is the keystroke
  // that gets taken back if the burst it opened turns out to be a scan.
  return {
    candidate: { chars: [key], lastAt: at, passed: 1 },
    step: { kind: "typing" },
  };
}

/** The element a leaked keystroke landed in, and what it held beforehand. */
interface Leak {
  readonly field: HTMLInputElement | HTMLTextAreaElement;
  readonly value: string;
  readonly selectionStart: number | null;
  readonly selectionEnd: number | null;
}

function editableTarget(target: EventTarget | null): Leak | null {
  if (
    !(target instanceof HTMLInputElement) &&
    !(target instanceof HTMLTextAreaElement)
  ) {
    return null;
  }
  return {
    field: target,
    value: target.value,
    selectionStart: target.selectionStart,
    selectionEnd: target.selectionEnd,
  };
}

/**
 * Put a field back the way it was before the keystroke this module let through.
 *
 * Restoring the snapshot rather than trimming a character is deliberate: the
 * leaked keystroke may have replaced a selection, in which case the text to put
 * back is not a suffix of anything still on screen.
 *
 * **A controlled React input will not notice this.** Assigning `value` does not
 * run React's synthetic `change` path, so a field whose text lives in component
 * state would re-render back over the restoration. No such field exists yet —
 * the sale screen is 1.11.5 — and the screen that introduces one owns the
 * fix, by routing its search box's state through the same handler that
 * receives the scan.
 */
function retract(leak: Leak): void {
  leak.field.value = leak.value;
  if (leak.selectionStart !== null && leak.selectionEnd !== null) {
    leak.field.setSelectionRange(leak.selectionStart, leak.selectionEnd);
  }
}

export interface ScanCaptureOptions {
  /** Called once per committed burst, with the characters between the gaps. */
  readonly onScan: (code: string) => void;
}

/**
 * Listen for scans anywhere under `root`, whatever has focus.
 *
 * The listener is registered in the **capture** phase, which is what makes the
 * whole thing possible: capture runs before the event reaches the focused
 * control, so `preventDefault()` there stops the character being inserted at
 * all. In the bubble phase the text has already landed.
 *
 * Returns the function that removes it.
 */
export function attachScanCapture(
  root: Document | HTMLElement,
  { onScan }: ScanCaptureOptions,
): () => void {
  let candidate = NO_SCAN_CANDIDATE;
  let leak: Leak | null = null;

  const onKeyDown = (event: Event): void => {
    if (!(event instanceof KeyboardEvent) || event.defaultPrevented) {
      return;
    }
    // A chord is a command, not a barcode. No scanner holds Control down.
    if (event.ctrlKey || event.altKey || event.metaKey) {
      candidate = NO_SCAN_CANDIDATE;
      leak = null;
      return;
    }

    const advanced = feedScanKey(candidate, event.key, event.timeStamp);
    candidate = advanced.candidate;

    switch (advanced.step.kind) {
      case "ignored":
        return;
      case "typing":
        // Snapshot before the default action runs, so the value recorded is the
        // one from before this keystroke.
        leak = editableTarget(event.target);
        return;
      case "absorbed":
        event.preventDefault();
        return;
      case "scanned": {
        event.preventDefault();
        if (advanced.step.leaked > 0 && leak !== null) {
          retract(leak);
        }
        leak = null;
        onScan(advanced.step.code);
        return;
      }
    }
  };

  root.addEventListener("keydown", onKeyDown, true);
  return () => {
    root.removeEventListener("keydown", onKeyDown, true);
  };
}
