#!/usr/bin/env bash
# Creates the "POS delivery" GitHub Project (Projects v2) and its custom fields.
#
# Projects v2 is free on a personal account and works with private repositories,
# so unlike branch protection this one is fully available today. It needs a token
# scope the default `gh auth login` does not grant:
#
#   gh auth refresh -s project,read:project
#
# Idempotent: existing fields must match the reviewed types and select options;
# the script adds only missing fields and refuses ambiguous or drifted schemas.
#
# HONEST LIMIT: the GitHub API can create a project and its fields, but it CANNOT
# create views (the saved board/table tabs). Those are four clicks each, once, and
# this script prints the exact recipe at the end.
# `./scripts/test-gh-setup.sh` exercises its API failure paths without GitHub.
set -euo pipefail

if [ "$#" -ne 0 ]; then
  echo "usage: $0" >&2
  exit 2
fi

die() {
  printf 'gh-project: %s\n' "$*" >&2
  exit 1
}

ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || {
  echo "gh-project: run this command inside the repository." >&2
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
  cat <<TXT
This script needs the 'project' token scope, which the default login does not grant.

  gh auth refresh -s project,read:project

That opens a browser once. Then run this script again.
TXT
  exit 1
fi

echo "owner: $OWNER"

if ! num=$(gh project list --owner "$OWNER" --closed --limit 1000 --format json \
      | "$PYTHON" -c '
import json
import sys

title = sys.argv[1]
projects = json.load(sys.stdin).get("projects", [])
matches = [str(project["number"]) for project in projects if project.get("title") == title]
if len(matches) > 1:
    raise SystemExit(f"duplicate projects named {title!r}; resolve them before rerunning")
print(matches[0] if matches else "")
' "$TITLE"); then
  die "failed to list projects; refusing to create a possible duplicate"
fi

if [ -z "$num" ]; then
  echo "creating project '$TITLE'"
  if ! num=$(gh project create --owner "$OWNER" --title "$TITLE" --format json \
        | "$PYTHON" -c 'import sys,json;print(json.load(sys.stdin)["number"])'); then
    die "failed to create project '$TITLE'"
  fi
  [ -n "$num" ] && [ "$num" != "null" ] \
    || die "GitHub returned an empty project number after creation"
  echo "  created #$num"
else
  echo "project '$TITLE' already exists as #$num"
fi

FIELD_QUERY='query($owner: String!, $number: Int!, $endCursor: String) {
  user(login: $owner) {
    projectV2(number: $number) { ...ProjectFields }
  }
  organization(login: $owner) {
    projectV2(number: $number) { ...ProjectFields }
  }
}
fragment ProjectFields on ProjectV2 {
  fields(first: 100, after: $endCursor) {
    nodes {
      __typename
      ... on ProjectV2FieldCommon { name dataType }
      ... on ProjectV2SingleSelectField { options { name } }
    }
    pageInfo { hasNextPage endCursor }
  }
}'

# `gh project field-list` exposes names and broad GraphQL types, but not enough
# information to prove that TEXT/DATE fields and single-select options match the
# reviewed delivery schema. Query the underlying schema and normalize every page
# before deciding whether an existing field is safe to reuse.
if ! existing=$(gh api graphql --paginate --slurp \
      -f query="$FIELD_QUERY" -F owner="$OWNER" -F number="$num" \
      | "$PYTHON" -c '
import json
import sys

pages = json.load(sys.stdin)
if not isinstance(pages, list) or not pages:
    raise SystemExit("GitHub returned no project-field pages")

fields = []
for page in pages:
    data = page.get("data") if isinstance(page, dict) else None
    if not isinstance(data, dict):
        raise SystemExit("GitHub returned an invalid project-field response")
    owners = [entry for entry in (data.get("user"), data.get("organization")) if entry is not None]
    if len(owners) != 1:
        raise SystemExit("project owner did not resolve uniquely as a user or organization")
    project = owners[0].get("projectV2") if isinstance(owners[0], dict) else None
    connection = project.get("fields") if isinstance(project, dict) else None
    nodes = connection.get("nodes") if isinstance(connection, dict) else None
    if not isinstance(nodes, list):
        raise SystemExit("GitHub returned an invalid project-field connection")
    for node in nodes:
        if not isinstance(node, dict) or not isinstance(node.get("__typename"), str):
            raise SystemExit("GitHub returned an invalid project-field node")
        name = node.get("name")
        if not isinstance(name, str):
            continue
        typename = node["__typename"]
        field_type = node.get("dataType")
        if not isinstance(field_type, str):
            raise SystemExit(f"field {name!r} has no data type")
        if typename == "ProjectV2SingleSelectField":
            raw_options = node.get("options")
            if not isinstance(raw_options, list) or not all(
                isinstance(option, dict) and isinstance(option.get("name"), str)
                for option in raw_options
            ):
                raise SystemExit(f"field {name!r} has invalid single-select options")
            options = [option["name"] for option in raw_options]
        else:
            options = []
        fields.append({"name": name, "type": field_type, "options": options})

print(json.dumps(fields, separators=(",", ":")))
'); then
  die "failed to inspect the field schema for project #$num; no field was changed"
fi

inspect_field() {  # inspect_field <name> <TYPE> [options]
  local name="$1" type="$2" opts="${3:-}"
  printf '%s\n' "$existing" | "$PYTHON" -c '
import json
import sys

name, expected_type, raw_options = sys.argv[1:]
expected_options = raw_options.split(",") if raw_options else []
fields = json.load(sys.stdin)
matches = [field for field in fields if field.get("name") == name]
if not matches:
    raise SystemExit(3)
if len(matches) != 1:
    print(f"field {name!r} exists {len(matches)} times; resolve duplicate names manually", file=sys.stderr)
    raise SystemExit(4)
actual = matches[0]
if actual.get("type") != expected_type:
    print(
        f"field {name!r} has type {actual.get('"'"'type'"'"')!r}; expected {expected_type!r}",
        file=sys.stderr,
    )
    raise SystemExit(4)
if actual.get("options") != expected_options:
    print(
        f"field {name!r} has options {actual.get('"'"'options'"'"')!r}; expected {expected_options!r}",
        file=sys.stderr,
    )
    raise SystemExit(4)
' "$name" "$type" "$opts"
}

create_field() {  # create_field <name> <TYPE> [options]
  local name="$1" type="$2" opts="${3:-}"
  if [ -n "$opts" ]; then
    if ! gh project field-create "$num" --owner "$OWNER" --name "$name" \
        --data-type "$type" --single-select-options "$opts" >/dev/null; then
      die "failed to create field '$name'"
    fi
  else
    if ! gh project field-create "$num" --owner "$OWNER" --name "$name" \
        --data-type "$type" >/dev/null; then
      die "failed to create field '$name'"
    fi
  fi
  echo "  created $name"
}

# Phase and Group are what the plan is actually organised by — a board grouped by
# anything else does not answer "what is left before the Phase 1 gate?".
field_names=("Phase" "Group" "Microstep" "Priority" "Risk" "Blocked" "Target")
field_types=("SINGLE_SELECT" "TEXT" "TEXT" "SINGLE_SELECT" "SINGLE_SELECT" "SINGLE_SELECT" "DATE")
field_options=(
  "0 close-out,1 sellable MVP,2 money-grade,3 connected,4 depth,5 harden & launch"
  ""
  ""
  "P0,P1,P2"
  "money path,migration,security,compliance,immutable,none"
  "merchant answer,decision,hardware,not blocked"
  ""
)
field_states=()

# Complete the whole read-only audit before the first mutation. A late mismatch
# must not leave a partially created schema that a subsequent run has to repair.
for field_index in "${!field_names[@]}"; do
  field_state=0
  inspect_field \
    "${field_names[$field_index]}" \
    "${field_types[$field_index]}" \
    "${field_options[$field_index]}" \
    || field_state=$?
  case "$field_state" in
    0|3) field_states[$field_index]=$field_state ;;
    *)
      die "existing field '${field_names[$field_index]}' does not match the reviewed schema; correct it manually before rerunning"
      ;;
  esac
done

echo "fields"
for field_index in "${!field_names[@]}"; do
  if [ "${field_states[$field_index]}" -eq 0 ]; then
    echo "  exists  ${field_names[$field_index]}"
  else
    create_field \
      "${field_names[$field_index]}" \
      "${field_types[$field_index]}" \
      "${field_options[$field_index]}"
  fi
done

echo
cat <<TXT
Project ready: https://github.com/users/$OWNER/projects/$num

Three things the API cannot do, so do them once by hand:

1. LINK THE REPOSITORY — so new issues can be added from the issue page:
     project → ⋯ → Settings → Manage access / Linked repositories → add OmarSweiti/pos

2. THE FOUR VIEWS. Each is: "+ New view" then set layout and grouping.

   "Board — now"        Board,  group by Status, filter: -status:Done
                        The only view open while working. If it has more than one
                        card in progress, WIP is not 1 (workflow §4.1).

   "Phase plan"         Table,  group by Phase, sort by Microstep
                        Reading order. Answers "what is left before the gate?".

   "Blocked"            Table,  filter: -no:Blocked -Blocked:"not blocked"
                        Anything blocked for a week is a risk, not a task (§16).

   "Money & compliance" Table,  filter: Risk:"money path",Risk:compliance,Risk:migration
                        The rows where a mistake costs money rather than time.

3. AUTOMATION — free, and it removes the step everyone forgets:
     project → ⋯ → Workflows → enable
       "Item closed"          → set Status = Done
       "Pull request merged"  → set Status = Done
       "Auto-add to project"  → filter: is:issue is:open  (repo: OmarSweiti/pos)
TXT
