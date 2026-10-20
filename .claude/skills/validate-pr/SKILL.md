---
name: validate-pr
description: Validate a pull request in the running app against the hardware emulators and post screenshot evidence on it before handing it to the owner. Use for every pull request once it is open and its checks are green, and again after any push that changes runtime behaviour; a pull request that changes nothing at runtime still gets an evidence comment that says so.
---

# Validate a pull request, with evidence

The owner approves every pull request, and approves it on evidence rather than on a
description — `docs/implementation/03-github-workflow.md` §6. This is the step that
produces that evidence. Do it for **every** pull request, before telling the owner it is
ready: a pull request handed over without an evidence comment is not finished.

## When

- Once the pull request is open and its required checks are green.
- Again after any later push that changes runtime behaviour. The comment names one head
  commit; a newer head is not covered by it.

## Steps

1. **Validate the exact head.** Check out the pull request's branch at its head commit, with
   no tracked change (`git status` clean). The publisher refuses any other state, because
   the comment names that commit as the one validated.
2. **Turn each claim into an observable step.** Read the pull request's *What* section and
   the `Done when` line of every microstep it lands. Each claim becomes one step with an
   expected result you can see.
3. **Start from a fresh state, with synthetic data only.** `just db-local-reset` for the
   register database, `just db-reset` when the server is involved. Never real merchant or
   customer data.
4. **Start the emulators the change touches** — printer, cash drawer, scanner, customer
   display, scale, payment terminal — as `docs/implementation/ref/hardware-and-receipts.md`
   describes, with the device profiles it names. Where an emulator does not exist yet, use
   the in-process simulator in `crates/pos-hardware` and say so in the summary.
5. **Launch the app and drive each step:** `just dev-terminal` for the register,
   `just dev-backoffice` for the back office, `just dev-server` for the server.
6. **Capture one screenshot per step** into a directory outside the repository — the
   session scratchpad, or a temporary directory — named in order:
   `01-cart-with-two-lines.png`, `02-receipt-preview.png`. PNG or JPEG, at most 2 MB each,
   at most 20. A receipt is evidenced by the printer emulator's rendered image, not a
   photograph of a screen. On macOS, `screencapture -x <file>` captures the screen once the
   terminal has Screen Recording permission; any tool that writes a real PNG or JPEG will do.
7. **Inspect every image before publishing.** The repository is public and evidence is
   public forever. No real customer data, no PIN, no card number, no token or key, nothing
   from another window or a notification. Retake any image that shows one.
8. **Write `summary.md`** in the same directory: the commands and emulator profiles used,
   then each step with its expected and observed result, then anything not validated and
   why. A step that failed is reported as failed. Never claim a check that did not run.
9. **Preview, then publish.** `just evidence-check <pr> <directory>` validates everything
   and prints the comment without writing anything. `just evidence <pr> <directory>`
   pushes the images to `refs/evidence/pr-<n>`, confirms GitHub serves each one, and posts
   one comment naming the validated commit.
10. **If any step failed,** fix it, push, and repeat from step 1. The owner receives a pull
    request whose evidence is all green, or one whose failure is its stated subject.

## A pull request that changes nothing at runtime

Documents, policy and CI configuration have nothing to launch. Post the evidence comment
anyway, with no screenshots: `summary.md` says `Runtime change: none`, why, and which checks
ran instead — `just lint`, `just guards`, the documentation-link check. The publisher
refuses that form for a pull request touching runtime code under `apps/`, `crates/` or
`packages/`, or a dependency manifest, lockfile or toolchain pin — for a dependency bump, a
smoke run of the app is the evidence that it still works.

## What this skill never does

It never approves or merges the pull request — that is the owner's act — and it never
edits a screenshot to make a step look as if it passed.
