# Rulesets, as data

The three rulesets that enforce the branch flow were applied with `gh api` and
existed only as live GitHub configuration. Nothing in the repository recorded
them, so nobody could diff what was configured against what was intended, and a
deletion — accidental or otherwise — had nothing to restore from.

These files are that record: the **applicable configuration** of each live
ruleset, normalized to the payload shape the API accepts back.

| File | Target | What it does |
|---|---|---|
| [`development-flow.json`](development-flow.json) | `refs/heads/development` | pull request required, six checks, force-push and deletion blocked, squash **or** merge |
| [`staging-promotion.json`](staging-promotion.json) | `refs/heads/staging` | as above, but **merge commit only** — the first server-side guard against the squashed promotion that forked the branches once |
| [`tags-v-append-only.json`](tags-v-append-only.json) | `refs/tags/v*` | `update` and `deletion` blocked with **no bypass actor at all**, so it binds the maintainer too |

`main` is deliberately absent: it carries no ruleset. See
[`../../docs/implementation/03-github-workflow.md`](../../docs/implementation/03-github-workflow.md) §3.

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

- **These payloads have not been round-tripped against this repository.** They are
  normalized *from* the live API and are the documented POST shape, but no restore
  has been executed, so treat a first restore as a change to review rather than a
  known-good replay.
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
