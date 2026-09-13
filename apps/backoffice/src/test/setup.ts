/**
 * The back office's Testing Library cleanup.
 *
 * `@testing-library/react` installs its own `afterEach(cleanup)` only behind
 * `if (typeof afterEach === 'function')`, and vitest's `globals` is off across
 * this repository — every test imports `describe`, `expect` and `it`
 * explicitly. So the automatic hook never registered here, and each render in
 * a file accumulated in the same `document.body`. The *second* rendering test
 * in a file then fails with "Found multiple elements with the role …", which
 * reads as a bad selector rather than as a leaking harness.
 *
 * `setupFiles` rather than an import in each test file, for the reason the
 * register's config states: a file that forgot the import would fail as a bad
 * selector, which is the same symptom this fixes.
 *
 * This is deliberately NOT a copy of `apps/terminal/src/test/setup.ts`. That
 * harness also registers jest-dom's matchers, bridges a fake clock into
 * `user-event`, and exports `renderWithProviders` for the register's provider
 * tree. The back office declares neither `@testing-library/jest-dom` nor
 * `@tanstack/react-query` and boots no providers, so mirroring it would add
 * dependencies to close a cleanup gap. When a back-office screen needs a
 * provider tree or a matcher, that microstep adds it here with its own test.
 */

import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(cleanup);
