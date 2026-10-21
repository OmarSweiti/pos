# Rulesets, as data

The rulesets that enforce the branch flow were applied with `gh api` and existed
only as live GitHub configuration. Nothing in the repository recorded them, so
nobody could diff what was configured against what was intended, and a deletion —
accidental or otherwise — had nothing to restore from.

These files are that record: the **applicable configuration** of each live
ruleset, normalized to the payload shape the API accepts back.

| File | Target | What it does |
|---|---|---|
| [`development-flow.json`](development-flow.json) | `refs/heads/development` | pull request required, seven checks — the six below and `validation`, which waits for the evidence comment — force-push and deletion blocked, squash **or** merge |
| [`staging-promotion.json`](staging-promotion.json) | `refs/heads/staging` | as above, but six checks — `rust`, `guards`, `web`, `supply-chain`, `protected-paths`, `topology`; a promotion's changes were each validated on their own pull request — and **merge commit only** — the first server-side guard against the squashed promotion that forked the branches once |
| [`main-append-only.json`](main-append-only.json) | `refs/heads/main` | `deletion` and `non_fast_forward` only. **No required checks and no required pull request** — those two rules need none, so they could be applied while main's `ci.yml` still predates four of the six required jobs |
| [`tags-v-append-only.json`](tags-v-append-only.json) | `refs/tags/v*` | `update` and `deletion` blocked with **no bypass actor at all**, so it binds the maintainer too |
| [`code-owner-review.json`](code-owner-review.json) | `development`, `staging`, `main` | every pull request needs **one approving review from a code owner**; a new push dismisses it; every review thread must be resolved. It also makes `main` pull-request-only, which `main-append-only` alone did not |

`branches/main/protection` answers `404 Branch not protected`. That is the
*legacy* protection API and it cannot see a ruleset — it is not evidence that
`main` is unprotected. Read `rulesets`. See
[`../../docs/implementation/03-github-workflow.md`](../../docs/implementation/03-github-workflow.md) §3.

## Why `development` is strict and `staging` is not

`strict_required_status_checks_policy` — GitHub's "require branches to be up to
date before merging" — is `true` on `development-flow` and **`false` on
`staging-promotion`**. It was `false` on both, for a reason recorded nowhere,
which is the same gap these files exist to close. It was then briefly `true` on
both, which was wrong, and the second half of this section is why.

*Textual* collisions were already safe without it:
[`../../scripts/check-protected-paths.sh`](../../scripts/check-protected-paths.sh)
judges a pull request against the **merge base**, not the base tip, so a branch
that has merely fallen behind is not accused of deleting a migration the base
merged in the meantime. **Semantic** collisions are not safe. A pull request can
pass [`../../scripts/verify-schema.py`](../../scripts/verify-schema.py)'s
exact-ordered-parity check, or
[`../../scripts/check-test-catalog.py`](../../scripts/check-test-catalog.py)'s
arithmetic, against its own snapshot of the tree; the base then moves; both
branches merge clean; and the branch goes red on the post-merge push run, where
nothing is gating and the breakage is already shared. The migrations are
sequentially numbered — `0001_init.sql` through `0004_people_and_audit.sql` — so
two pull requests each adding `0005` each pass alone and collide only once both
are in. Dependabot is authorized for five cargo and five npm pull requests a
month against one `Cargo.lock` and one `pnpm-lock.yaml` under `--locked`, which
is the live, monthly version of exactly that risk.

The cost is real and worth stating: every work pull request that falls behind
now pays one update-and-rerun, and a rerun of `rust` is a full job, not a
seconds-long check.

### Why `staging` must stay `false`

The claim that replaced this paragraph said promotion pull requests are "current
by construction" and pay nothing. That is exactly backwards, and PR #148 proved
it: opened as `development → staging`, GitHub reported `mergeStateStatus: BEHIND`
and refused the merge.

The branch model is the reason. A promotion merges `development` into `staging`
with a **merge commit**, and that commit lives only on `staging` — it is never
merged back. So `staging` is ahead of `development` by one commit per promotion
already made, permanently and by design. `git rev-list --count
origin/development..origin/staging` is the live count, read there rather than
frozen into this sentence, which would go stale on the next promotion.
`handoff.md` states
the same property from the other side — *"'Synchronised' means the upstream tip
is an ancestor and the promoted trees agree — not that divergence counts are
zero."* Strict compares tips, not trees, so on `staging` it can never be
satisfied by a promotion; it can only be satisfied by back-merging `staging` into
`development` first, which would add a contentless merge commit to the default
branch before every single promotion.

And the risk it would buy there is nil. The semantic-collision argument above is
about **concurrent** pull requests racing a base that moves underneath them.
That happens on `development`, which takes feature branches and up to ten
Dependabot pull requests a month. `staging` has exactly one inbound route, taken
one promotion at a time, from a single branch. There is no race on `staging` to
protect against, so strict there costs a mandatory back-merge and prevents
nothing.

## Why approval is its own ruleset

`code-owner-review` was applied on **20 October 2026**, when the owner asked that
no pull request merge without their approval, Dependabot's included. It is
deliberately separate from the three branch rulesets, and the reason is the one
fact that shapes everything else here: **GitHub never lets an author approve their
own pull request.**

So the gate binds differently by author, and both halves are intended:

- **Anyone else's pull request** — the engineering app's, which opens every one an
  agent prepares, Dependabot's, a contributor's — needs the owner's approving
  review. `CODEOWNERS` maps every path to the owner, so GitHub requests that review
  automatically, and nothing but the approval satisfies the rule.
- **The owner's own pull request** — one the owner opened by hand — can never be
  approved. It merges through the administrator's `pull_request` bypass, and that
  deliberate merge **is** the owner's approval. GitHub records it as a bypass event.

That second case is why the approval lives here and not in `development-flow`.
A bypass is recorded per ruleset, so the owner's routine merge of their own green
pull request bypasses `code-owner-review` and nothing else: `development-flow`
evaluates as passed. A bypass of `development-flow` or `staging-promotion` still
means what it meant before — a merge with a required check red — and stays
distinguishable in rule insights instead of being buried under every ordinary
merge. Folding the approval into the branch rulesets would have made every merge
a bypass of the check wall too.

The parameters, and why each is set the way it is:

| Parameter | Value | Why |
|---|---|---|
| `required_approving_review_count` | `1` | one owner; one approval is the whole of the available review |
| `require_code_owner_review` | `true` | the approval must come from a code owner — today the owner, and still the owner the day a second collaborator with write access arrives |
| `dismiss_stale_reviews_on_push` | `true` | an approval covers the diff that was read. A push that changes it, including a Dependabot rebase that moves the lockfile, needs approving again |
| `required_review_thread_resolution` | `true` | a question the owner asked in review is answered before the merge, not after it |
| `require_last_push_approval` | `false` | with one person it only ever bites the owner: a fix-up the owner pushes onto a Dependabot branch would make that pull request unapprovable by the only approver |
| `require_extra_approval_for_unattributed_changes` | `true` | GitHub's default. Per [GitHub's rules reference](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets), it adds one approval to a pull request **Copilot** opens under its own app identity without a person attributed, and does nothing when zero approvals are required. Nothing here uses Copilot's coding agent, so it is inert — and if one ever opens a pull request, demanding a second approval is the right answer |
| `allowed_merge_methods` | `merge`, `squash` | the union the flow uses; GitHub applies the most restrictive layer, so `staging` stays merge-commit-only through `staging-promotion` |

Two limits, stated plainly. The rule binds **identities**, not people: an agent
holding the owner's credentials is indistinguishable from the owner to GitHub, so
what stops an agent approving or bypass-merging is the agent's own reviewed
settings — `.claude/settings.json` and `.codex/rules/safety.rules` — and not this
file. And `main` gains a pull-request requirement here but still requires **no
status check**; that wall arrives with the promotion that carries the current
`ci.yml` onto it.

## What the tag ruleset does not do

`tags-v-append-only` has exactly two rules, `deletion` and `update`, and an
empty `bypass_actors`. **`creation` is absent.** A correctly shaped, signed `v*`
tag can therefore still be *created* on any commit the branch rules allow —
including `main`'s head today — and once created it can never be moved or
deleted, by anyone. The ruleset makes a mistaken tag **irreversible, not
impossible**, and that is the whole of its guarantee.

Adding `{"type": "creation"}` is the only ruleset-level way to stop a tag from
being created. It would also require a bypass actor to ship at all, because tags
are not created through pull requests, and a bypass actor is precisely the
"binds the maintainer too" property the table above advertises. The trade is not
worth making. The control against a mistaken tag is therefore upstream of the
ruleset: the `guard` job in
[`../workflows/release.yml`](../workflows/release.yml), which refuses a tag whose
grammar, signature, or branch head is wrong before the platform matrix starts,
and the `staging → main` promotion that decides what a taggable head contains.

## Diff live against intended

```sh
for f in .github/rulesets/*.json; do
  name=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["name"])' "$f")
  id=$(gh api repos/:owner/:repo/rulesets --jq ".[]|select(.name==\"$name\")|.id")
  gh api "repos/:owner/:repo/rulesets/$id" \
    | python3 -c 'import json,sys;d=json.load(sys.stdin);k=("name","target","enforcement","conditions","bypass_actors","rules");p={x:d[x] for x in k if x in d};[r.pop("parameters",None) for r in p["rules"] if r.get("parameters") is None];p["rules"]=sorted(p["rules"],key=lambda r:r["type"]);print(json.dumps(p,indent=2,sort_keys=True,ensure_ascii=False))' \
    | diff -u "$f" - && echo "$name: matches"
done
```

## Restore or recreate one

```sh
gh api --method POST repos/:owner/:repo/rulesets --input .github/rulesets/<name>.json
```

To update an existing one in place, `PUT` to `repos/:owner/:repo/rulesets/<id>`.

## What is NOT claimed here

- **Four of these payloads have not been round-tripped against this repository.**
  They are normalized *from* the live API and are the documented POST shape, but no
  restore has been executed, so treat a first restore as a change to review rather
  than a known-good replay. `code-owner-review.json` is the exception: it was
  created on 20 October 2026 by POSTing this exact file, and the diff loop above
  read it back identical.
- **Nothing keeps these files and the live configuration in agreement.** No gate
  runs the diff above. Drift is detectable, not prevented — the check reaches the
  network, so it belongs in a job that already does, not in the local gate.
- **`scripts/gh-protect.sh` still refuses and exits 3.** It predates the rulesets
  API. Rewriting it to apply these files, with the negative tests
  `scripts/test-gh-setup.sh` requires, is the remaining work; until then applying a
  file is the manual command above.

Issue #115 tracked the original "no ruleset exists" gap and was closed when the
three were applied. The *diff and restore* half it also named was not closed by
that, which is what these files are for.
