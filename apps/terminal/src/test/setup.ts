/**
 * The register's DOM component-test harness (microstep 1.11.0).
 *
 * Three jobs, and each exists because leaving it out fails in a way that reads
 * as a broken test rather than a broken harness:
 *
 *  1. `renderWithProviders` renders through the same provider tree `main.tsx`
 *     boots, so a screen test exercises the component in its real context.
 *  2. `cleanup` runs after every test. `@testing-library/react` registers that
 *     hook itself only when a global `afterEach` exists, and vitest's `globals`
 *     is deliberately off here — the repository's tests import `describe`,
 *     `expect` and `it` explicitly. Without this line every render inside one
 *     file accumulates in the same `document.body`, and the *second* rendering
 *     test in a file fails with "Found multiple elements with the role …".
 *  3. jest-dom's matchers are registered from `/vitest`, not the bare
 *     specifier: the bare entry point ends in a top-level `expect.extend(…)`
 *     with no import of `expect`, so under `globals: false` it throws
 *     `ReferenceError: expect is not defined` at import time.
 *
 * This file is a `.ts`, as the microstep's `Files:` line names it, so the
 * provider tree is built with `createElement` rather than JSX. Vite 8 runs on
 * Oxc, which refuses JSX in a `.ts` file and offers no per-file loader
 * override.
 */

import "@testing-library/jest-dom/vitest";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  type RenderOptions,
  type RenderResult,
  render,
} from "@testing-library/react";
import { createElement, type ReactElement, StrictMode } from "react";
import { afterEach, vi } from "vitest";

afterEach(cleanup);

/**
 * `@testing-library/react` decides whether to advance a fake clock inside its
 * async wrapper by testing for a global `jest`, which vitest never defines
 * (`pure.js`'s inlined `jestFakeTimersAreEnabled`). The consequence is not a
 * degraded mode: with `vi.useFakeTimers()` active, RTL's async wrapper awaits a
 * `setTimeout(…, 0)` that the frozen clock never fires, so **every**
 * `await user.*()` hangs until the test times out.
 *
 * The microstep this harness belongs to promises the opposite — "scan-burst
 * tests use fake timers and `user-event`'s `advanceTimers`, so the `< 30 ms`
 * heuristic is deterministic rather than scheduler-dependent" — so the harness
 * owes the bridge. Only `advanceTimersByTime` is needed: it is the sole `jest.*`
 * member either testing-library package calls.
 *
 * `vi.useFakeTimers({ shouldAdvanceTime: true })` also unhangs the await, and
 * is the wrong fix: it advances `Date.now()` across an `await` with no explicit
 * advance, which is exactly the scheduler dependence fake timers are here to
 * remove.
 *
 * A call site still passes `advanceTimers` to `userEvent.setup()`. `user-event`
 * calls `config.advanceTimers` itself, and it is a noop by default.
 */
Object.defineProperty(globalThis, "jest", {
  configurable: true,
  value: {
    advanceTimersByTime: (ms: number) => {
      vi.advanceTimersByTime(ms);
    },
  },
});

/**
 * A client per render. `main.tsx` builds one at module scope, which is right
 * for an application and wrong for a suite: a shared cache makes the second
 * test's first paint the first test's data, so a test passes or fails on
 * execution order and reads as a race.
 *
 * `retry` is off because the default of three, with exponential backoff, spends
 * about seven seconds before a query settles — past every timeout in the stack,
 * and reported as whichever one fires first rather than as a retry.
 */
function testQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
}

/**
 * Render `ui` inside the register's provider tree.
 *
 * The tree is supplied as `wrapper` rather than wrapped around `ui`, so the
 * returned `rerender` keeps the providers instead of dropping them.
 */
export function renderWithProviders(
  ui: ReactElement,
  options?: Omit<RenderOptions, "wrapper">,
): RenderResult {
  const client = testQueryClient();
  return render(ui, {
    ...options,
    wrapper: ({ children }) =>
      createElement(
        StrictMode,
        null,
        createElement(QueryClientProvider, { client }, children),
      ),
  });
}
