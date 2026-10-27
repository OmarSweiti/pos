#!/usr/bin/env bash
# Deterministic failure-path tests for the GitHub setup scripts. No request
# reaches GitHub: a minimal `gh` double records every attempted operation.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || {
  echo "test-gh-setup: run inside the repository" >&2
  exit 1
}
TMP=$(mktemp -d "${TMPDIR:-/tmp}/pos-gh-setup.XXXXXX")
trap 'rm -rf "$TMP"' EXIT HUP INT TERM
mkdir -p "$TMP/bin"

MOCK_LOG="$TMP/gh.log"
OUTPUT="$TMP/output"
STATUS=0
PASSED=0
FAILED=0

cat >"$TMP/bin/gh" <<'MOCK_GH'
#!/usr/bin/env bash
set -u

printf '%s\n' "$*" >>"$MOCK_GH_LOG"

if [ "${1:-}" = "auth" ] && [ "${2:-}" = "status" ]; then
  if [ "$MOCK_GH_SCENARIO" = "auth_failure" ]; then
    echo "mock authentication failure" >&2
    exit 40
  fi
  if [ "$MOCK_GH_SCENARIO" = "item_no_project_scope" ]; then
    echo "Token scopes: 'repo'"
    exit 0
  fi
  echo "Token scopes: 'repo', 'project', 'read:project'"
  exit 0
fi

if [ "${1:-}" = "pr" ] && [ "${2:-}" = "view" ]; then
  case "$MOCK_GH_SCENARIO" in
    item_pr_missing)
      echo "mock pr-view failure" >&2
      exit 51
      ;;
    item_closed)
      printf '%s\n' '{"url":"https://github.com/test/pos/pull/12","number":12,"title":"chore(repo): abandoned  [—]","state":"CLOSED","labels":[]}'
      ;;
    item_no_step)
      printf '%s\n' '{"url":"https://github.com/test/pos/pull/12","number":12,"title":"chore(repo): bump a tool  [—]","state":"MERGED","labels":[{"name":"type: chore"}]}'
      ;;
    item_unlabelled)
      printf '%s\n' '{"url":"https://github.com/test/pos/pull/12","number":12,"title":"feat(domain): several components  [1.3.5]","state":"OPEN","labels":[]}'
      ;;
    item_range)
      printf '%s\n' '{"url":"https://github.com/test/pos/pull/12","number":12,"title":"feat(db): across two groups  [1.2.9–1.3.1]","state":"MERGED","labels":[{"name":"risk: security"},{"name":"risk: migration"}]}'
      ;;
    *)
      printf '%s\n' '{"url":"https://github.com/test/pos/pull/12","number":12,"title":"feat(domain): several \"components\", stacked  [1.3.5]","state":"OPEN","labels":[{"name":"type: feat"},{"name":"risk: security"},{"name":"risk: money path"}]}'
      ;;
  esac
  exit 0
fi

if [ "${1:-}" = "repo" ] && [ "${2:-}" = "view" ]; then
  if [ "$MOCK_GH_SCENARIO" = "repo_failure" ]; then
    echo "mock repository lookup failure" >&2
    exit 41
  fi
  echo "test/pos"
  exit 0
fi

if [ "${1:-}" = "label" ] && [ "${2:-}" = "create" ]; then
  if [ "$MOCK_GH_SCENARIO" = "label_failure" ]; then
    echo "mock label failure" >&2
    exit 42
  fi
  exit 0
fi

if [ "${1:-}" = "project" ]; then
  case "${2:-}" in
    list)
      if [ "$MOCK_GH_SCENARIO" = "project_list_failure" ]; then
        echo "mock project-list failure" >&2
        exit 43
      fi
      case "$MOCK_GH_SCENARIO" in
        project_create_failure|item_board_missing)
          printf '{"projects":[]}\n'
          ;;
        *)
          printf '{"projects":[{"title":"POS delivery","number":7}]}\n'
          ;;
      esac
      exit 0
      ;;
    create)
      if [ "$MOCK_GH_SCENARIO" = "project_create_failure" ]; then
        echo "mock project-create failure" >&2
        exit 44
      fi
      printf '{"number":7}\n'
      exit 0
      ;;
    item-add)
      if [ "$MOCK_GH_SCENARIO" = "item_add_failure" ]; then
        echo "mock item-add failure" >&2
        exit 50
      fi
      printf '{"id":"PVTI_mock"}\n'
      exit 0
      ;;
    field-create)
      if [ "$MOCK_GH_SCENARIO" = "field_create_failure" ]; then
        echo "mock field-create failure" >&2
        exit 46
      fi
      exit 0
      ;;
  esac
fi

if [ "${1:-}" = "api" ] && [ "${2:-}" = "graphql" ]; then
  case "$*" in
    *"fragment Board"*)
      case "$MOCK_GH_SCENARIO" in
        item_board_unreadable)
          echo "mock board read failure" >&2
          exit 53
          ;;
        item_field_missing)
          printf '%s\n' '{"data":{"repositoryOwner":{"projectV2":{"id":"PVT_mock","fields":{"nodes":[{"id":"F_status","name":"Status","dataType":"SINGLE_SELECT","options":[{"id":"O_todo","name":"Todo"},{"id":"O_progress","name":"In Progress"},{"id":"O_done","name":"Done"}]},{"id":"F_phase","name":"Phase","dataType":"SINGLE_SELECT","options":[{"id":"O_p0","name":"0 close-out"},{"id":"O_p1","name":"1 sellable MVP"},{"id":"O_p2","name":"2 money-grade"}]},{"id":"F_group","name":"Group","dataType":"TEXT"},{"id":"F_risk","name":"Risk","dataType":"SINGLE_SELECT","options":[{"id":"O_money","name":"money path"},{"id":"O_immutable","name":"immutable"},{"id":"O_migration","name":"migration"},{"id":"O_compliance","name":"compliance"},{"id":"O_security","name":"security"},{"id":"O_none","name":"none"}]},{}]}}}}}'
          ;;
        item_option_missing)
          printf '%s\n' '{"data":{"repositoryOwner":{"projectV2":{"id":"PVT_mock","fields":{"nodes":[{"id":"F_status","name":"Status","dataType":"SINGLE_SELECT","options":[{"id":"O_todo","name":"Todo"},{"id":"O_done","name":"Done"}]},{"id":"F_phase","name":"Phase","dataType":"SINGLE_SELECT","options":[{"id":"O_p0","name":"0 close-out"},{"id":"O_p1","name":"1 sellable MVP"},{"id":"O_p2","name":"2 money-grade"}]},{"id":"F_group","name":"Group","dataType":"TEXT"},{"id":"F_microstep","name":"Microstep","dataType":"TEXT"},{"id":"F_risk","name":"Risk","dataType":"SINGLE_SELECT","options":[{"id":"O_money","name":"money path"},{"id":"O_immutable","name":"immutable"},{"id":"O_migration","name":"migration"},{"id":"O_compliance","name":"compliance"},{"id":"O_security","name":"security"},{"id":"O_none","name":"none"}]},{}]}}}}}'
          ;;
        *)
          printf '%s\n' '{"data":{"repositoryOwner":{"projectV2":{"id":"PVT_mock","fields":{"nodes":[{"id":"F_status","name":"Status","dataType":"SINGLE_SELECT","options":[{"id":"O_todo","name":"Todo"},{"id":"O_progress","name":"In Progress"},{"id":"O_done","name":"Done"}]},{"id":"F_phase","name":"Phase","dataType":"SINGLE_SELECT","options":[{"id":"O_p0","name":"0 close-out"},{"id":"O_p1","name":"1 sellable MVP"},{"id":"O_p2","name":"2 money-grade"}]},{"id":"F_group","name":"Group","dataType":"TEXT"},{"id":"F_microstep","name":"Microstep","dataType":"TEXT"},{"id":"F_risk","name":"Risk","dataType":"SINGLE_SELECT","options":[{"id":"O_money","name":"money path"},{"id":"O_immutable","name":"immutable"},{"id":"O_migration","name":"migration"},{"id":"O_compliance","name":"compliance"},{"id":"O_security","name":"security"},{"id":"O_none","name":"none"}]},{}]}}}}}'
          ;;
      esac
      exit 0
      ;;
    *"--input -"*)
      cat >"$MOCK_GH_LOG.body"
      if [ "$MOCK_GH_SCENARIO" = "item_update_failure" ]; then
        echo "mock field-update failure" >&2
        exit 52
      fi
      printf '{"data":{}}\n'
      exit 0
      ;;
  esac
fi

if [ "${1:-}" = "api" ]; then
  case " $* " in
    *" api graphql "*)
      for required in --paginate --slurp ProjectV2FieldCommon dataType options pageInfo hasNextPage endCursor; do
        case "$*" in
          *"$required"*) ;;
          *) echo "project-field query omitted required contract: $required" >&2; exit 96 ;;
        esac
      done
      if [ "$MOCK_GH_SCENARIO" = "field_personal_owner" ]; then
        case "$*" in
          *"organization(login:"*)
            printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}},"organization":null},"errors":[{"type":"NOT_FOUND","path":["organization"],"message":"Could not resolve to an Organization with the login of '\''OmarSweiti'\''."}]}]'
            echo "gh: Could not resolve to an Organization with the login of 'OmarSweiti'." >&2
            exit 1
            ;;
        esac
      fi
      if [ "$MOCK_GH_SCENARIO" = "field_organization_owner" ]; then
        case "$*" in
          *"user(login:"*)
            printf '%s\n' '[{"data":{"user":null,"organization":{"projectV2":{"fields":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}},"errors":[{"type":"NOT_FOUND","path":["user"],"message":"Could not resolve to a User with the login of '\''OmarSweiti'\''."}]}]'
            echo "gh: Could not resolve to a User with the login of 'OmarSweiti'." >&2
            exit 1
            ;;
        esac
      fi
      if [ "$MOCK_GH_SCENARIO" = "field_schema_failure" ]; then
        echo "mock project-field schema failure" >&2
        exit 45
      fi
      case "$MOCK_GH_SCENARIO" in
        field_personal_owner)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[]}}}}}]'
          ;;
        field_organization_owner)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"Organization","projectV2":{"fields":{"nodes":[]}}}}}]'
          ;;
        field_owner_missing)
          printf '%s\n' '[{"data":{"repositoryOwner":null}}]'
          ;;
        field_owner_unknown)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"Enterprise","projectV2":{"fields":{"nodes":[]}}}}}]'
          ;;
        field_project_missing)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User"}}}]'
          ;;
        project_happy)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[{"name":"0 close-out"},{"name":"1 sellable MVP"},{"name":"2 money-grade"},{"name":"3 connected"},{"name":"4 depth"},{"name":"5 harden & launch"}]},{"__typename":"ProjectV2Field","name":"Group","dataType":"TEXT"},{"__typename":"ProjectV2Field","name":"Microstep","dataType":"TEXT"},{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Priority","options":[{"name":"P0"},{"name":"P1"},{"name":"P2"}]},{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Risk","options":[{"name":"money path"},{"name":"migration"},{"name":"security"},{"name":"compliance"},{"name":"immutable"},{"name":"none"}]},{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Blocked","options":[{"name":"merchant answer"},{"name":"decision"},{"name":"hardware"},{"name":"not blocked"}]},{"__typename":"ProjectV2Field","name":"Target","dataType":"DATE"}]}}}}}]'
          ;;
        field_wrong_type)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2Field","name":"Phase","dataType":"TEXT"}]}}}}}]'
          ;;
        field_late_mismatch)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2Field","name":"Group","dataType":"DATE"},{"__typename":"ProjectV2Field","name":"Risk","dataType":"TEXT"},{"__typename":"ProjectV2Field","name":"Target","dataType":"TEXT"}]}}}}}]'
          ;;
        field_wrong_select_kind)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2MultiSelectField","name":"Phase","dataType":"MULTI_SELECT"}]}}}}}]'
          ;;
        field_duplicate)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[]},{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[]}]}}}}}]'
          ;;
        field_missing_option)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[{"name":"0 close-out"},{"name":"1 sellable MVP"},{"name":"2 money-grade"},{"name":"3 connected"},{"name":"4 depth"}] }]}}}}}]'
          ;;
        field_extra_option)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[{"name":"0 close-out"},{"name":"1 sellable MVP"},{"name":"2 money-grade"},{"name":"3 connected"},{"name":"4 depth"},{"name":"5 harden & launch"},{"name":"6 unsupported"}]}]}}}}}]'
          ;;
        *)
          printf '%s\n' '[{"data":{"repositoryOwner":{"__typename":"User","projectV2":{"fields":{"nodes":[]}}}}}]'
          ;;
      esac
      exit 0
      ;;
    *"milestones?state=all&per_page=100"*)
      if [ "$MOCK_GH_SCENARIO" = "milestone_list_failure" ]; then
        echo "mock milestone-list failure" >&2
        exit 47
      fi
      if [ "$MOCK_GH_SCENARIO" = "bootstrap_existing" ]; then
        printf '[[{"title":"Phase 0 — close-out","number":1},{"title":"Phase 1 — sellable MVP","number":2},{"title":"Phase 2 — money-grade","number":3},{"title":"Phase 3 — connected","number":4},{"title":"Phase 4 — depth","number":5},{"title":"Phase 5 — harden & launch","number":6}]]\n'
      else
        printf '[[]]\n'
      fi
      exit 0
      ;;
    *" -X PATCH repos/test/pos -F allow_squash_merge=true "*)
      if [ "$MOCK_GH_SCENARIO" = "merge_failure" ]; then
        echo "mock merge-settings failure" >&2
        exit 48
      fi
      printf '{}\n'
      exit 0
      ;;
    *" api repos/test/pos --jq .default_branch "*)
      if [ "$MOCK_GH_SCENARIO" = "default_branch_failure" ]; then
        echo "mock default-branch failure" >&2
        exit 49
      fi
      echo "development"
      exit 0
      ;;
    *)
      printf '{}\n'
      exit 0
      ;;
  esac
fi

echo "unexpected mock gh invocation: $*" >&2
exit 97
MOCK_GH
chmod +x "$TMP/bin/gh"

run_case() {
  local scenario="$1" script="$2"
  shift 2
  : >"$MOCK_LOG"
  : >"$OUTPUT"
  rm -f "$MOCK_LOG.body"
  set +e
  PATH="$TMP/bin:$PATH" \
    MOCK_GH_SCENARIO="$scenario" \
    MOCK_GH_LOG="$MOCK_LOG" \
    bash "$ROOT/$script" "$@" >"$OUTPUT" 2>&1
  STATUS=$?
  set -e
}

pass() {
  PASSED=$((PASSED + 1))
  printf '  PASS  %s\n' "$1"
}

fail() {
  FAILED=$((FAILED + 1))
  printf '  FAIL  %s\n' "$1" >&2
}

expect_success() {
  if [ "$STATUS" -eq 0 ]; then pass "$1"; else fail "$1 (exit $STATUS)"; fi
}

expect_failure() {
  if [ "$STATUS" -ne 0 ]; then pass "$1"; else fail "$1 (unexpected success)"; fi
}

expect_output() {
  local description="$1" pattern="$2"
  if grep -qF -- "$pattern" "$OUTPUT"; then pass "$description"; else fail "$description"; fi
}

expect_no_output() {
  local description="$1" pattern="$2"
  if grep -qF -- "$pattern" "$OUTPUT"; then fail "$description"; else pass "$description"; fi
}

expect_body() {
  local description="$1" pattern="$2"
  if [ -f "$MOCK_LOG.body" ] && grep -qF -- "$pattern" "$MOCK_LOG.body"; then
    pass "$description"
  else
    fail "$description"
  fi
}

expect_no_body() {
  local description="$1" pattern="$2"
  if [ -f "$MOCK_LOG.body" ] && grep -qF -- "$pattern" "$MOCK_LOG.body"; then
    fail "$description"
  else
    pass "$description"
  fi
}

expect_no_call() {
  local description="$1" pattern="$2"
  if grep -qF -- "$pattern" "$MOCK_LOG"; then fail "$description"; else pass "$description"; fi
}

echo "gh-bootstrap failure semantics"
run_case normal scripts/gh-bootstrap.sh --dry-run
expect_success "dry-run succeeds"
expect_no_output "dry-run does not claim auto-merge" "auto-merge"
expect_no_call "dry-run performs no mutation" "-X "
expect_no_call "dry-run does not call label creation" "label create"

run_case repo_failure scripts/gh-bootstrap.sh
expect_failure "repository lookup failure is fatal"
expect_no_output "repository failure never prints Done" "Done."

run_case label_failure scripts/gh-bootstrap.sh
expect_failure "label API failure is fatal"
expect_no_output "label failure never prints Done" "Done."

run_case milestone_list_failure scripts/gh-bootstrap.sh
expect_failure "milestone list failure is fatal"
expect_output "milestone list failure explains duplicate protection" "refusing to create a possible duplicate"
expect_no_call "failed milestone list cannot create a milestone" "-X POST repos/test/pos/milestones"
expect_no_output "milestone list failure never prints Done" "Done."

run_case merge_failure scripts/gh-bootstrap.sh
expect_failure "merge-settings API failure is fatal"
expect_no_output "merge-settings failure never prints Done" "Done."

run_case default_branch_failure scripts/gh-bootstrap.sh
expect_failure "default-branch API failure is fatal"
expect_no_output "default-branch failure never prints Done" "Done."

run_case bootstrap_existing scripts/gh-bootstrap.sh
expect_success "existing repository setup is idempotent"
expect_no_call "existing milestones are updated, not duplicated" "-X POST repos/test/pos/milestones"
expect_output "successful bootstrap prints Done" "Done."

echo
echo "gh-project failure semantics"
run_case auth_failure scripts/gh-project.sh
expect_failure "authentication-status failure is fatal"
expect_no_output "authentication failure never prints ready" "Project ready:"

run_case project_list_failure scripts/gh-project.sh
expect_failure "project list failure is fatal"
expect_output "project list failure explains duplicate protection" "refusing to create a possible duplicate"
expect_no_call "failed project list cannot create a project" "project create"
expect_no_output "project list failure never prints ready" "Project ready:"

run_case project_create_failure scripts/gh-project.sh
expect_failure "project create failure is fatal"
expect_no_output "project create failure never prints ready" "Project ready:"

run_case field_schema_failure scripts/gh-project.sh
expect_failure "field-schema inspection failure is fatal"
expect_no_call "failed field-schema inspection cannot create a field" "project field-create"
expect_no_output "field-schema failure never prints ready" "Project ready:"

run_case field_personal_owner scripts/gh-project.sh
expect_success "a personal-account owner resolves through the repository-owner interface"
expect_output "personal-account resolution reaches field creation" "  created Target"
expect_output "personal-account resolution prints ready" "Project ready:"

run_case field_organization_owner scripts/gh-project.sh
expect_success "an organization owner resolves through the repository-owner interface"
expect_output "organization-owner resolution reaches field creation" "  created Target"
expect_output "organization-owner resolution prints ready" "Project ready:"

run_case field_owner_missing scripts/gh-project.sh
expect_failure "an unresolved repository owner is fatal"
expect_output "an unresolved repository owner is explained" "project owner login did not resolve as a user or organization"
expect_no_call "an unresolved repository owner cannot create a field" "project field-create"

run_case field_owner_unknown scripts/gh-project.sh
expect_failure "an unsupported repository-owner type is fatal"
expect_output "an unsupported repository-owner type is explained" "project owner resolved with unsupported type 'Enterprise'"
expect_no_call "an unsupported repository-owner type cannot create a field" "project field-create"

run_case field_project_missing scripts/gh-project.sh
expect_failure "a missing project under the resolved owner is fatal"
expect_output "a missing project under the resolved owner is explained" "projectV2 did not resolve for the repository owner"
expect_no_call "a missing project under the resolved owner cannot create a field" "project field-create"

run_case field_wrong_type scripts/gh-project.sh
expect_failure "an existing field with the wrong data type is fatal"
expect_no_call "wrong-type fields are not duplicated or mutated" "project field-create"
expect_no_output "wrong-type fields never print ready" "Project ready:"

run_case field_late_mismatch scripts/gh-project.sh
expect_failure "a late schema mismatch is fatal even when an earlier field is missing"
expect_no_call "the full field preflight completes before any missing field is created" "project field-create"
expect_no_output "late field mismatches never print ready" "Project ready:"

run_case field_wrong_select_kind scripts/gh-project.sh
expect_failure "a same-name multi-select cannot impersonate a single-select field"
expect_no_call "wrong select kinds are not duplicated or mutated" "project field-create"
expect_no_output "wrong select kinds never print ready" "Project ready:"

run_case field_duplicate scripts/gh-project.sh
expect_failure "duplicate same-name fields are fatal"
expect_no_call "duplicate fields are not mutated" "project field-create"
expect_no_output "duplicate fields never print ready" "Project ready:"

run_case field_missing_option scripts/gh-project.sh
expect_failure "a single-select field with a missing option is fatal"
expect_no_call "incomplete single-select fields are not mutated" "project field-create"
expect_no_output "incomplete select fields never print ready" "Project ready:"

run_case field_extra_option scripts/gh-project.sh
expect_failure "a single-select field with an extra option is fatal"
expect_no_call "unexpected single-select options are not mutated" "project field-create"
expect_no_output "unexpected select options never print ready" "Project ready:"

run_case field_create_failure scripts/gh-project.sh
expect_failure "field create failure is fatal"
expect_no_output "field create failure never prints ready" "Project ready:"

run_case project_happy scripts/gh-project.sh
expect_success "existing complete project is idempotent"
expect_no_call "existing complete project is reused rather than duplicated" "project create"
expect_no_call "complete project creates no duplicate fields" "project field-create"
expect_output "successful project setup prints ready" "Project ready:"

echo
echo "gh-project-item failure semantics"
run_case normal scripts/gh-project-item.sh
expect_failure "no pull request is a usage error"
expect_output "the usage error names the argument" "usage:"

run_case auth_failure scripts/gh-project-item.sh 12
expect_failure "authentication failure is fatal"
expect_no_call "authentication failure adds nothing" "project item-add"

run_case item_no_project_scope scripts/gh-project-item.sh 12
expect_failure "a login without the project scope is refused"
expect_output "the refusal says how to grant the scope" "gh auth refresh -s project,read:project"
expect_no_call "a login without the scope adds nothing" "project item-add"

run_case item_board_missing scripts/gh-project-item.sh 12
expect_failure "a board that does not resolve is fatal"
expect_output "a missing board points at its bootstrap" "run just gh-project"
expect_no_call "a missing board adds nothing" "project item-add"

run_case item_board_unreadable scripts/gh-project-item.sh 12
expect_failure "an unreadable board is fatal"
expect_no_call "an unreadable board adds nothing" "project item-add"

run_case item_pr_missing scripts/gh-project-item.sh 12
expect_failure "a pull request that does not resolve is fatal"
expect_no_call "an unresolved pull request adds nothing" "project item-add"

run_case item_closed scripts/gh-project-item.sh 12
expect_failure "a pull request closed without merging is refused"
expect_output "the refusal says why" "closed without merging"
expect_no_call "a closed pull request is not added" "project item-add"

run_case item_field_missing scripts/gh-project-item.sh 12
expect_failure "a board missing a field is refused"
expect_output "the refusal names the field" "no 'Microstep' field"
expect_no_call "a drifted board is refused before anything is added" "project item-add"

run_case item_option_missing scripts/gh-project-item.sh 12
expect_failure "a board missing an option is refused"
expect_output "the refusal names the option" "no option 'In Progress'"
expect_no_call "a missing option is refused before anything is added" "project item-add"

run_case item_add_failure scripts/gh-project-item.sh 12
expect_failure "an add failure is fatal"
expect_no_call "an add failure sets no field" "graphql --input -"
expect_no_output "an add failure never reports a card" "board: #"

run_case item_update_failure scripts/gh-project-item.sh 12
expect_failure "a field failure is fatal"
expect_output "a field failure says the card was added but not set" "failed to set its fields"
expect_no_output "a field failure never reports a card" "board: #"

run_case normal scripts/gh-project-item.sh 12
expect_success "an open pull request is filed"
expect_output "it is in progress, with its step and its most consequential risk" "board: #12 → In Progress · 1 sellable MVP · 1.3 · 1.3.5 · money path"
expect_body "the card is the one the add returned" '"item": "PVTI_mock"'
expect_body "a select value is the board's own option id" '{"singleSelectOptionId": "O_progress"}'
expect_body "a text value travels as a variable" '{"text": "1.3.5"}'
expect_no_body "no value is written into the query text" 'value: {'

run_case item_unlabelled scripts/gh-project-item.sh 12
expect_success "an unlabelled pull request is filed"
expect_output "an unlabelled pull request has no Risk yet" "board: #12 → In Progress · 1 sellable MVP · 1.3 · 1.3.5"
expect_no_body "no Risk is set before the labeler has run" "F_risk"

run_case item_no_step scripts/gh-project-item.sh 12
expect_success "a merged pull request naming no step is filed"
expect_output "it is done, with no risk and no step" "board: #12 → Done · none"
expect_no_body "a title naming no step sets no Phase" "F_phase"

run_case item_range scripts/gh-project-item.sh 12
expect_success "a range of steps is filed"
expect_output "a range across groups keeps both, and the gravest risk" "board: #12 → Done · 1 sellable MVP · 1.2–1.3 · 1.2.9–1.3.1 · migration"

echo
printf 'GitHub setup self-test: %d passed, %d failed\n' "$PASSED" "$FAILED"
[ "$FAILED" -eq 0 ]
