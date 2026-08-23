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
  echo "Token scopes: 'repo', 'project', 'read:project'"
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
        project_create_failure)
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
    field-create)
      if [ "$MOCK_GH_SCENARIO" = "field_create_failure" ]; then
        echo "mock field-create failure" >&2
        exit 46
      fi
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
      if [ "$MOCK_GH_SCENARIO" = "field_schema_failure" ]; then
        echo "mock project-field schema failure" >&2
        exit 45
      fi
      case "$MOCK_GH_SCENARIO" in
        project_happy)
          printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[{"name":"0 close-out"},{"name":"1 sellable MVP"},{"name":"2 money-grade"},{"name":"3 connected"},{"name":"4 depth"},{"name":"5 harden & launch"}]},{"__typename":"ProjectV2Field","name":"Group","dataType":"TEXT"},{"__typename":"ProjectV2Field","name":"Microstep","dataType":"TEXT"},{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Priority","options":[{"name":"P0"},{"name":"P1"},{"name":"P2"}]},{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Risk","options":[{"name":"money path"},{"name":"migration"},{"name":"security"},{"name":"compliance"},{"name":"immutable"},{"name":"none"}]},{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Blocked","options":[{"name":"merchant answer"},{"name":"decision"},{"name":"hardware"},{"name":"not blocked"}]},{"__typename":"ProjectV2Field","name":"Target","dataType":"DATE"}]}}},"organization":null}}]'
          ;;
        field_wrong_type)
          printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2Field","name":"Phase","dataType":"TEXT"}]}}},"organization":null}}]'
          ;;
        field_late_mismatch)
          printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2Field","name":"Group","dataType":"DATE"},{"__typename":"ProjectV2Field","name":"Risk","dataType":"TEXT"},{"__typename":"ProjectV2Field","name":"Target","dataType":"TEXT"}]}}},"organization":null}}]'
          ;;
        field_wrong_select_kind)
          printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2MultiSelectField","name":"Phase","dataType":"MULTI_SELECT"}]}}},"organization":null}}]'
          ;;
        field_duplicate)
          printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[]},{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[]}]}}},"organization":null}}]'
          ;;
        field_missing_option)
          printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[{"name":"0 close-out"},{"name":"1 sellable MVP"},{"name":"2 money-grade"},{"name":"3 connected"},{"name":"4 depth"}] }]}}},"organization":null}}]'
          ;;
        field_extra_option)
          printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","dataType":"SINGLE_SELECT","name":"Phase","options":[{"name":"0 close-out"},{"name":"1 sellable MVP"},{"name":"2 money-grade"},{"name":"3 connected"},{"name":"4 depth"},{"name":"5 harden & launch"},{"name":"6 unsupported"}]}]}}},"organization":null}}]'
          ;;
        *)
          printf '%s\n' '[{"data":{"user":{"projectV2":{"fields":{"nodes":[]}}},"organization":null}}]'
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
expect_no_call "complete project creates no duplicate fields" "project field-create"
expect_output "successful project setup prints ready" "Project ready:"

echo
printf 'GitHub setup self-test: %d passed, %d failed\n' "$PASSED" "$FAILED"
[ "$FAILED" -eq 0 ]
