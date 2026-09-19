/**
 * The keyboard map (microstep 1.11.11).
 *
 * `ref/ui-spec.md` §7 is the normative table and it is short on purpose —
 * "memorisable, and printed on a card taped to the register". Ten rows, and
 * `:247` states the obligation they exist to meet: *"Every action is reachable
 * without a mouse."* A cashier's hands are on a keyboard and a scanner, and
 * `:15` notes that a register frequently has no mouse to reach for at all.
 *
 * The map is **data**, and the dispatcher takes its handlers as arguments. That
 * is the same discipline `direction.ts` applies to the document root and
 * `scanner.ts` to its clock: none of the actions below exists yet — pay, park,
 * resume and returns are screens nobody has written — and a module that reached
 * for them could not be tested until they did.
 *
 * ## Three transcription decisions, each of which could have been silent
 *
 * **`Del` and `Esc` are `KeyboardEvent.key` values `Delete` and `Escape`.** The
 * spec writes the label printed on the key; the DOM reports something else.
 *
 * **The spec's minus is `−`, U+2212.** That is the typographic minus sign, and
 * no keyboard emits it — a binding on it would never fire, and would look
 * correct in review because it matches the document character for character.
 * The binding is on `-`, U+002D, which is what both the main row and the numpad
 * produce. `ref/hardware-and-receipts.md:335` is the reason this module keys off
 * the character produced rather than a physical scancode: wedge scanners are
 * qualified against "the Arabic keyboard layout", so the physical position of a
 * key is not stable across the layouts this product runs under.
 *
 * **The phase file's prose lists eight bindings; `ui-spec.md` §7 has ten.** The
 * phase file omits `Esc` and `Enter`. It is summarising rather than narrowing —
 * "every action reachable" is not satisfied by a map missing cancel — so all ten
 * are here.
 *
 * ## Why some keys yield to a focused field
 *
 * A key that means something inside a text box belongs to the text box while one
 * has focus. `+` in the search field is a plus sign, not a quantity change;
 * `Delete` is a character deletion, not a voided line. Function keys and
 * `Escape` have no such meaning and fire wherever focus is.
 *
 * This is the same problem `scanner.ts` solves for bursts and it has the same
 * shape, but not the same answer: a scan is recognisable by its timing and can
 * be taken back out of the field it leaked into, whereas a single `+` is
 * indistinguishable from a cashier typing one. So this module yields rather than
 * retracts.
 *
 * ## `Enter` is shared with the scan capture, and the order matters
 *
 * `scanner.ts` listens in the **capture** phase and calls `preventDefault` on
 * `Enter` only when that Enter commits a burst. This listener runs in the
 * **bubble** phase and skips any event already handled, so a scan's terminator
 * never also fires `confirm`, while a bare `Enter` still does. `ui-spec.md:245`
 * gives that key both jobs — "confirm / commit scan" — and this is the seam
 * where the two are kept apart.
 */

/** Every action `ref/ui-spec.md` §7 binds. */
export type Action =
  | "search"
  | "pay"
  | "park"
  | "resume"
  | "returns"
  | "voidLine"
  | "quantityIncrement"
  | "quantityDecrement"
  | "lock"
  | "cancel"
  | "confirm";

/**
 * Whether a binding yields to a focused text field.
 *
 * `global` keys have no meaning inside a text box and always fire. `typed` keys
 * do, and belong to the box while it has focus.
 */
export type BindingScope = "global" | "typed";

export interface Binding {
  readonly action: Action;
  readonly scope: BindingScope;
}

/**
 * `ref/ui-spec.md` §7, transcribed. The quantity row binds two keys, because
 * "+ / −  quantity" is one row describing two directions.
 */
export const KEY_BINDINGS: ReadonlyMap<string, Binding> = new Map<
  string,
  Binding
>([
  ["F2", { action: "search", scope: "global" }],
  ["F4", { action: "pay", scope: "global" }],
  ["F6", { action: "park", scope: "global" }],
  ["F7", { action: "resume", scope: "global" }],
  ["F9", { action: "returns", scope: "global" }],
  ["F12", { action: "lock", scope: "global" }],
  ["Escape", { action: "cancel", scope: "global" }],
  ["Delete", { action: "voidLine", scope: "typed" }],
  ["+", { action: "quantityIncrement", scope: "typed" }],
  ["-", { action: "quantityDecrement", scope: "typed" }],
  ["Enter", { action: "confirm", scope: "typed" }],
]);

/** Handlers for the actions a screen implements. An absent one is inert. */
export type KeymapHandlers = Partial<Record<Action, () => void>>;

/**
 * The action a keystroke should run, or `null` for one this map ignores.
 *
 * `editing` is whether a text field currently has focus — an argument rather
 * than something read from a document, so every rule above is decidable without
 * a DOM.
 */
export function actionFor(
  key: string,
  { editing, chord }: { readonly editing: boolean; readonly chord: boolean },
): Action | null {
  // A chord is an application or OS command, not a till action. No row of
  // `ui-spec.md` §7 carries a modifier.
  if (chord) {
    return null;
  }
  const binding = KEY_BINDINGS.get(key);
  if (binding === undefined) {
    return null;
  }
  if (binding.scope === "typed" && editing) {
    return null;
  }
  return binding.action;
}

/** Whether `target` is somewhere a keystroke becomes text. */
export function isEditable(target: EventTarget | null): boolean {
  if (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement
  ) {
    return true;
  }
  return target instanceof HTMLElement && target.isContentEditable;
}

/**
 * Run `handlers` from the keyboard, anywhere under `root`.
 *
 * Registered in the **bubble** phase and skipping any already-handled event, so
 * `scanner.ts`'s capture-phase listener resolves a scan's `Enter` first. Returns
 * the function that removes it.
 */
export function attachKeymap(
  root: Document | HTMLElement,
  handlers: KeymapHandlers,
): () => void {
  const onKeyDown = (event: Event): void => {
    if (!(event instanceof KeyboardEvent) || event.defaultPrevented) {
      return;
    }
    const action = actionFor(event.key, {
      editing: isEditable(event.target),
      chord: event.ctrlKey || event.altKey || event.metaKey,
    });
    if (action === null) {
      return;
    }
    const handler = handlers[action];
    if (handler === undefined) {
      // Mapped but unimplemented: leave the key alone rather than swallowing
      // it, so an unwired screen degrades to the browser's behaviour instead of
      // to silence.
      return;
    }
    event.preventDefault();
    handler();
  };

  root.addEventListener("keydown", onKeyDown);
  return () => {
    root.removeEventListener("keydown", onKeyDown);
  };
}
