## Promotion

<!-- development → staging (release candidate), or staging → main (production). -->

**From → to:** `<source branch>` → `<destination branch>`
**Pull request:** `<URL or #number>`
**Reviewed base SHA:** `<full 40-character destination-tip SHA>`
**Reviewed head SHA:** `<full 40-character SHA>`
**Intended tag:** `<vX.Y.Z-rc.N, vX.Y.Z, or n/a>`

## What is in this promotion

<!-- The groups that landed since the last promotion. Use: git log --oneline <destination>..<source> -->

## What is deliberately NOT in it

<!-- Anything held back, and why. -->

## Evidence

- [ ] `gh pr view <PR> --json headRefName,baseRefName,baseRefOid,headRefOid` matches the
      source, destination, and both full reviewed SHAs above
- [ ] `bash ./scripts/watch-pr-checks.sh <PR>` exited successfully for that exact PR snapshot
- [ ] immediately before merge, re-read `baseRefOid` and `headRefOid`; both still equal the
      reviewed SHAs above, otherwise discard this evidence and re-run the watcher
- [ ] the ten-minute smoke, on a fresh database — §5.9
- [ ] the drills that apply to this change — §5.10
- [ ] the Arabic + RTL pass on every screen this touched — §5.3
- [ ] the keyboard-only pass — §5.4
- [ ] a restore from backup was actually performed, not merely possible — §5.10

## Release notes

<!-- Plain sentences a merchant would understand. This becomes the release body. -->

## Rollback

<!-- The exact way back. Tags are append-only by policy: a bad build is a new patch, never a moved tag. Published releases lock their associated tag and assets. -->

---

**Merge with a merge commit, never a squash.** Squashing replaces the source commits with a new
commit on the destination, breaks the shared ancestry this flow depends on, and makes later
promotions propose the same commits again. Pass the reviewed head through `--match-head-commit`.
That option atomically locks only the head; the current plan/API has no atomic target-base lock.
Serialize maintainer merges—or temporarily freeze the destination branch—during the final
base/head recheck and merge window. The recheck narrows but cannot eliminate that residual race.
