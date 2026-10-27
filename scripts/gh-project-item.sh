#!/usr/bin/env bash
# Files one pull request on the "POS delivery" board (03-github-workflow.md §5).
#
# `just pr` runs it on the pull request it has just opened, and again once the
# checks are green and the labeler has run; `just gh-project-item <pr>` runs it
# by hand. It sets only what the pull request already says about itself:
#
#   Status     In Progress while open, Done once merged. A pull request closed
#              without merging is refused: it has no outcome to track.
#   Phase      from the title's step: [1.3.5] is "1 sellable MVP"
#   Group      "1.3"; a range across groups is "1.2–1.3"
#   Microstep  the step as the title writes it: 1.3.5, 1.2.4b, 1.3.2–1.3.5
#   Risk       the most consequential `risk:` label, or "none" once the labeler
#              has labelled the pull request at all
#
# A title ending in [—] names no step, so Phase, Group and Microstep stay
# empty. A field is never cleared, and Priority, Blocked and Target are a
# person's to set.
#
# It runs as the owner's own login, which needs the 'project' scope
# (gh auth refresh -s project,read:project). The engineering app that opens the
# pull request carries no project permission, by design.
#
# Adding an item that is already on the board returns that item, so a second run
# updates the card rather than duplicating it. Two writes per run, every field
# in one of them: GitHub limits writes, and a backfill of every pull request has
# to fit. `./scripts/test-gh-setup.sh` exercises its failure paths without
# GitHub.
set -euo pipefail

if [ "$#" -ne 1 ] || [ -z "$1" ]; then
  echo "usage: $0 <pull-request number or URL>" >&2
  exit 2
fi
ref=$1

die() {
  printf 'gh-project-item: %s\n' "$*" >&2
  exit 1
}

ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || {
  echo "gh-project-item: run this command inside the repository." >&2
  exit 1
}
PYTHON="$ROOT/scripts/run-python.sh"

OWNER="${POS_GH_OWNER:-OmarSweiti}"
TITLE="POS delivery"

if ! auth_status=$(gh auth status 2>&1); then
  printf '%s\n' "$auth_status" >&2
  die "could not verify GitHub authentication"
fi
if ! grep -q 'project' <<<"$auth_status"; then
  die "the login lacks the 'project' scope: run gh auth refresh -s project,read:project"
fi

if ! num=$(gh project list --owner "$OWNER" --limit 1000 --format json \
      | "$PYTHON" -c '
import json
import sys

title = sys.argv[1]
projects = json.load(sys.stdin).get("projects", [])
matches = [str(project["number"]) for project in projects if project.get("title") == title]
if len(matches) != 1:
    raise SystemExit(f"expected one open project named {title!r}; found {len(matches)}")
print(matches[0])
' "$TITLE"); then
  die "the board did not resolve; run just gh-project"
fi

PROJECT_QUERY='query($owner: String!, $number: Int!) {
  repositoryOwner(login: $owner) {
    ... on User { projectV2(number: $number) { ...Board } }
    ... on Organization { projectV2(number: $number) { ...Board } }
  }
}
fragment Board on ProjectV2 {
  id
  fields(first: 100) {
    nodes {
      ... on ProjectV2FieldCommon { id name dataType }
      ... on ProjectV2SingleSelectField { options { id name } }
    }
  }
}'
if ! board=$(gh api graphql -f query="$PROJECT_QUERY" -F owner="$OWNER" -F number="$num"); then
  die "failed to read the fields of project #$num"
fi

if ! pr=$(gh pr view "$ref" --json url,number,title,state,labels); then
  die "pull request $ref did not resolve"
fi

# Everything the card will say, decided before anything is written.
if ! plan=$(BOARD="$board" PR="$pr" "$PYTHON" -c '
import json
import os
import re

owner = json.loads(os.environ["BOARD"]).get("data", {}).get("repositoryOwner") or {}
project = owner.get("projectV2")
if not isinstance(project, dict) or not project.get("id"):
    raise SystemExit("the project did not resolve for its owner")
fields = {node["name"]: node for node in project["fields"]["nodes"] if node.get("name")}
pr = json.loads(os.environ["PR"])
number = pr["number"]

PHASES = {
    "0": "0 close-out",
    "1": "1 sellable MVP",
    "2": "2 money-grade",
    "3": "3 connected",
    "4": "4 depth",
    "5": "5 harden & launch",
}
# Most consequential first: money moves, then a completed sale, then the schema.
RISKS = ["money path", "immutable", "migration", "compliance", "security"]
STEP = r"[0-5]\.[0-9]+\.[0-9]+[a-z]?"

if pr["state"] == "CLOSED":
    raise SystemExit(f"#{number} was closed without merging; it has no outcome to track")
values = {"Status": "Done" if pr["state"] == "MERGED" else "In Progress"}

step = re.search(rf"\[({STEP}(?:–{STEP})?)\]$", pr["title"].rstrip())
if step:
    microstep = step.group(1)
    groups = [".".join(end.split(".")[:2]) for end in microstep.split("–")]
    values["Phase"] = PHASES[microstep[0]]
    values["Group"] = groups[0] if len(set(groups)) == 1 else "–".join(groups)
    values["Microstep"] = microstep

labels = [label["name"] for label in pr["labels"]]
if labels:
    risks = [risk for risk in RISKS if f"risk: {risk}" in labels]
    values["Risk"] = risks[0] if risks else "none"

updates = []
for name, value in values.items():
    field = fields.get(name)
    if field is None:
        raise SystemExit(f"the board has no {name!r} field; run just gh-project")
    kind = field.get("dataType")
    if kind == "SINGLE_SELECT":
        option = [o["id"] for o in field.get("options", []) if o["name"] == value]
        if len(option) != 1:
            raise SystemExit(f"the board field {name!r} has no option {value!r}; run just gh-project")
        updates.append([field["id"], {"singleSelectOptionId": option[0]}])
    elif kind == "TEXT":
        updates.append([field["id"], {"text": value}])
    else:
        raise SystemExit(f"the board field {name!r} is {kind}, not a select or text")

print(json.dumps({
    "project": project["id"],
    "url": pr["url"],
    "number": number,
    "updates": updates,
    "summary": " · ".join(values.values()),
}))
'); then
  die "nothing was written for $ref"
fi

field_of() {  # field_of <key>: one value from the plan
  PLAN="$plan" "$PYTHON" -c 'import json,os,sys; print(json.loads(os.environ["PLAN"])[sys.argv[1]])' "$1"
}
url=$(field_of url)

if ! item=$(gh project item-add "$num" --owner "$OWNER" --url "$url" --format json \
      | "$PYTHON" -c 'import json,sys; print(json.load(sys.stdin)["id"])'); then
  die "failed to add $url to project #$num"
fi

# Every field in one request, each value a variable rather than query text.
body=$(PLAN="$plan" ITEM="$item" "$PYTHON" -c '
import json
import os

plan = json.loads(os.environ["PLAN"])
declared = ["$project: ID!", "$item: ID!"]
calls = []
variables = {"project": plan["project"], "item": os.environ["ITEM"]}
for index, (field, value) in enumerate(plan["updates"]):
    declared += [f"$field{index}: ID!", f"$value{index}: ProjectV2FieldValue!"]
    calls.append(
        f"set{index}: updateProjectV2ItemFieldValue(input: {{projectId: $project, itemId: $item, "
        f"fieldId: $field{index}, value: $value{index}}}) {{ clientMutationId }}"
    )
    variables[f"field{index}"] = field
    variables[f"value{index}"] = value
query = "mutation(" + ", ".join(declared) + ") { " + " ".join(calls) + " }"
print(json.dumps({"query": query, "variables": variables}))
')
if ! printf '%s' "$body" | gh api graphql --input - >/dev/null; then
  die "added $url to project #$num, but failed to set its fields"
fi

printf 'board: #%s → %s\n' "$(field_of number)" "$(field_of summary)"
