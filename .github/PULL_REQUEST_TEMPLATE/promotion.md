## Promotion

<!-- development → staging (release candidate), or staging → main (production). -->

**From → to:** `development` → `staging`
**Intended tag:** `v0.2.0-rc.1`

## What is in this promotion

<!-- The groups that landed since the last promotion. `git log --oneline staging..development` -->

## What is deliberately NOT in it

<!-- Anything held back, and why. -->

## Evidence

- [ ] CI green on the head branch — `gh run list --branch development --limit 1`
- [ ] the ten-minute smoke, on a fresh database — §5.9
- [ ] the drills that apply to this change — §5.10
- [ ] the Arabic + RTL pass on every screen this touched — §5.3
- [ ] the keyboard-only pass — §5.4
- [ ] a restore from backup was actually performed, not merely possible — §5.10

## Release notes

<!-- Plain sentences a merchant would understand. This becomes the release body. -->

## Rollback

<!-- The exact way back. A tag is immutable: a bad build is a new patch, never a moved tag. -->

---

**Merge with a merge commit, never a squash.** Squashing a promotion PR forks `staging` from
`development` permanently and every later promotion shows the same commits again.
