#!/usr/bin/env bash
# Creates the "POS delivery" GitHub Project (Projects v2) and its custom fields.
#
# Projects v2 is free on a personal account and works with private repositories,
# so unlike branch protection this one is fully available today. It needs a token
# scope the default `gh auth login` does not grant:
#
#   gh auth refresh -s project,read:project
#
# Idempotent: if the project already exists it only adds missing fields.
#
# HONEST LIMIT: the GitHub API can create a project and its fields, but it CANNOT
# create views (the saved board/table tabs). Those are four clicks each, once, and
# this script prints the exact recipe at the end.
set -uo pipefail

OWNER="${POS_GH_OWNER:-OmarSweiti}"
TITLE="POS delivery"

if ! gh auth status 2>&1 | grep -q 'project'; then
  cat <<TXT
This script needs the 'project' token scope, which the default login does not grant.

  gh auth refresh -s project,read:project

That opens a browser once. Then run this script again.
TXT
  exit 1
fi

echo "owner: $OWNER"

num=$(gh project list --owner "$OWNER" --format json 2>/dev/null \
      | python3 -c 'import sys,json;d=json.load(sys.stdin);print(next((str(p["number"]) for p in d.get("projects",[]) if p["title"]=="'"$TITLE"'"),""))' 2>/dev/null)

if [ -z "$num" ]; then
  echo "creating project '$TITLE'"
  num=$(gh project create --owner "$OWNER" --title "$TITLE" --format json \
        | python3 -c 'import sys,json;print(json.load(sys.stdin)["number"])') || exit 1
  echo "  created #$num"
else
  echo "project '$TITLE' already exists as #$num"
fi

existing=$(gh project field-list "$num" --owner "$OWNER" --format json 2>/dev/null \
           | python3 -c 'import sys,json;[print(f["name"]) for f in json.load(sys.stdin).get("fields",[])]')

field() {  # field <name> <TYPE> [options]
  local name="$1" type="$2" opts="${3:-}"
  if printf '%s\n' "$existing" | grep -qxF "$name"; then
    echo "  exists  $name"; return
  fi
  if [ -n "$opts" ]; then
    gh project field-create "$num" --owner "$OWNER" --name "$name" \
      --data-type "$type" --single-select-options "$opts" >/dev/null 2>&1
  else
    gh project field-create "$num" --owner "$OWNER" --name "$name" \
      --data-type "$type" >/dev/null 2>&1
  fi
  [ $? -eq 0 ] && echo "  created $name" || echo "  FAILED  $name"
}

echo "fields"
# Phase and Group are what the plan is actually organised by — a board grouped by
# anything else does not answer "what is left before the Phase 1 gate?".
field "Phase"     SINGLE_SELECT "0 close-out,1 sellable MVP,2 money-grade,3 connected,4 depth,5 harden & launch"
field "Group"     TEXT
field "Microstep" TEXT
field "Priority"  SINGLE_SELECT "P0,P1,P2"
field "Risk"      SINGLE_SELECT "money path,migration,security,compliance,immutable,none"
field "Blocked"   SINGLE_SELECT "merchant answer,decision,hardware,not blocked"
field "Target"    DATE

echo
cat <<TXT
Project ready: https://github.com/users/$OWNER/projects/$num

Two things the API cannot do, so do them once by hand:

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
