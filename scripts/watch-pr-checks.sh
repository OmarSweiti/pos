#!/usr/bin/env bash

# Wait for every check that this repository requires for one immutable PR
# snapshot, then watch that exact PR to completion. GitHub registers independent
# workflows asynchronously, so seeing one check is not evidence that the full
# required set exists yet.

set -euo pipefail

readonly REGISTRATION_ATTEMPTS=60
readonly REGISTRATION_DELAY_SECONDS=2

usage() {
  cat >&2 <<'EOF'
usage: scripts/watch-pr-checks.sh <PR-number-or-URL>
       scripts/watch-pr-checks.sh --self-test

The script derives the required workflow/job pairs from the PR base, head, and
complete changed-path list. It refuses a changed PR snapshot and waits for all
required checks to register before invoking `gh pr checks --watch`.
EOF
}

security_workflow_required() {
  local base_ref=$1 path
  shift

  case "$base_ref" in
    development|staging|main) ;;
    *) return 1 ;;
  esac

  for path in "$@"; do
    case "$path" in
      .github/*|scripts/check-branch-workflow-policy.rb|scripts/gh-actions-policy.sh|scripts/install-gitleaks-ci.sh|scripts/scan-secrets.sh|.gitleaks.toml)
        return 0
        ;;
    esac
  done
  return 1
}

promotion_notice_required() {
  case "$1" in
    development|staging|main|hotfix/*) return 0 ;;
    *) return 1 ;;
  esac
}

cross_platform_required() {
  case "$1" in
    staging|main) return 0 ;;
    *) return 1 ;;
  esac
}

expected_checks() {
  local base_ref=$1 head_ref=$2
  shift 2

  printf '%s\n' \
    "ci"$'\t'"rust" \
    $'ci\tguards' \
    $'ci\tweb' \
    $'ci\tsupply-chain' \
    $'branch-flow\tprotected-paths' \
    $'branch-flow\ttopology'

  if security_workflow_required "$base_ref" "$@"; then
    printf '%s\n' $'security\tworkflow-analysis'
  fi

  if cross_platform_required "$base_ref"; then
    printf '%s\n' \
      $'ci\tcross-platform (ubuntu-22.04)' \
      $'ci\tcross-platform (macos-latest)' \
      $'ci\tcross-platform (windows-latest)'
  fi

  if promotion_notice_required "$head_ref"; then
    printf '%s\n' $'branch-flow\tpromotion-notice'
  fi
}

contains_exact_line() {
  local haystack=$1 needle=$2 line
  while IFS= read -r line; do
    [ "$line" = "$needle" ] && return 0
  done <<< "$haystack"
  return 1
}

missing_checks() {
  local expected=$1 actual=$2 check
  while IFS= read -r check; do
    [ -n "$check" ] || continue
    contains_exact_line "$actual" "$check" || printf '%s\n' "$check"
  done <<< "$expected"
}

display_check() {
  local row=$1 workflow name
  IFS=$'\t' read -r workflow name <<< "$row"
  printf '%s / %s' "$workflow" "$name"
}

assert_has_check() {
  local description=$1 rows=$2 check=$3
  SELF_TESTS=$((SELF_TESTS + 1))
  if ! contains_exact_line "$rows" "$check"; then
    printf 'not ok %d - %s (missing %s)\n' \
      "$SELF_TESTS" "$description" "$(display_check "$check")" >&2
    SELF_TEST_FAILURES=$((SELF_TEST_FAILURES + 1))
    return
  fi
  printf 'ok %d - %s\n' "$SELF_TESTS" "$description"
}

assert_lacks_check() {
  local description=$1 rows=$2 check=$3
  SELF_TESTS=$((SELF_TESTS + 1))
  if contains_exact_line "$rows" "$check"; then
    printf 'not ok %d - %s (unexpected %s)\n' \
      "$SELF_TESTS" "$description" "$(display_check "$check")" >&2
    SELF_TEST_FAILURES=$((SELF_TEST_FAILURES + 1))
    return
  fi
  printf 'ok %d - %s\n' "$SELF_TESTS" "$description"
}

assert_complete() {
  local description=$1 expected=$2 actual=$3 missing
  SELF_TESTS=$((SELF_TESTS + 1))
  missing=$(missing_checks "$expected" "$actual")
  if [ -n "$missing" ]; then
    printf 'not ok %d - %s (required checks remain missing)\n%s\n' \
      "$SELF_TESTS" "$description" "$missing" >&2
    SELF_TEST_FAILURES=$((SELF_TEST_FAILURES + 1))
    return
  fi
  printf 'ok %d - %s\n' "$SELF_TESTS" "$description"
}

assert_incomplete() {
  local description=$1 expected=$2 actual=$3 missing
  SELF_TESTS=$((SELF_TESTS + 1))
  missing=$(missing_checks "$expected" "$actual")
  if [ -z "$missing" ]; then
    printf 'not ok %d - %s (incomplete set was accepted)\n' \
      "$SELF_TESTS" "$description" >&2
    SELF_TEST_FAILURES=$((SELF_TEST_FAILURES + 1))
    return
  fi
  printf 'ok %d - %s\n' "$SELF_TESTS" "$description"
}

assert_equal_value() {
  local description=$1 expected=$2 actual=$3
  SELF_TESTS=$((SELF_TESTS + 1))
  if [ "$actual" != "$expected" ]; then
    printf 'not ok %d - %s (expected %q, got %q)\n' \
      "$SELF_TESTS" "$description" "$expected" "$actual" >&2
    SELF_TEST_FAILURES=$((SELF_TEST_FAILURES + 1))
    return
  fi
  printf 'ok %d - %s\n' "$SELF_TESTS" "$description"
}

canonical_workflow_key() {
  local path=$1 event=$2 workflow_name=$3
  if [ "$path" = '.github/workflows/ci.yml' ] && \
     [ "$event" = 'pull_request' ] && [ "$workflow_name" = 'ci' ]; then
    printf '%s\n' 'ci'
    return
  fi
  if [ "$path" = '.github/workflows/branch-flow.yml' ] && \
     [ "$event" = 'pull_request_target' ] && [ "$workflow_name" = 'branch-flow' ]; then
    printf '%s\n' 'branch-flow'
    return
  fi
  if [ "$path" = '.github/workflows/security.yml' ] && \
     [ "$event" = 'pull_request' ] && [ "$workflow_name" = 'security' ]; then
    printf '%s\n' 'security'
    return
  fi
  return 1
}

row_state_accepted() {
  local mode=$1 state=$2
  case "$mode" in
    registered) return 0 ;;
    successful) [ "$state" = 'SUCCESS' ] ;;
    *) return 2 ;;
  esac
}

workflow_contract_valid() {
  ruby -rpsych <<'RUBY'
def load_workflow(path)
  Psych.safe_load_file(path, aliases: false)
rescue Psych::Exception, SystemCallError => error
  warn "#{path}: #{error.message}"
  exit 1
end

ci = load_workflow(".github/workflows/ci.yml")
abort "ci.yml workflow name changed" unless ci["name"] == "ci"
ci_jobs = ci.fetch("jobs")
required_ci_jobs = %w[rust guards web supply-chain cross-platform]
abort "ci.yml readiness job set changed" unless ci_jobs.keys == required_ci_jobs
matrix = ci_jobs.fetch("cross-platform").dig("strategy", "matrix", "platform")
expected_matrix = %w[ubuntu-22.04 macos-latest windows-latest]
abort "ci.yml cross-platform matrix changed" unless matrix == expected_matrix
expected_matrix_condition =
  "github.base_ref == 'staging' || github.base_ref == 'main' || " \
  "github.ref == 'refs/heads/staging' || github.ref == 'refs/heads/main'"
unless ci_jobs.fetch("cross-platform").fetch("if") == expected_matrix_condition
  abort "ci.yml cross-platform route condition changed"
end

branch_flow = load_workflow(".github/workflows/branch-flow.yml")
abort "branch-flow.yml workflow name changed" unless branch_flow["name"] == "branch-flow"
expected_branch_jobs = %w[protected-paths topology promotion-notice]
unless branch_flow.fetch("jobs").keys == expected_branch_jobs
  abort "branch-flow.yml readiness job set changed"
end

security = load_workflow(".github/workflows/security.yml")
abort "security.yml workflow name changed" unless security["name"] == "security"
abort "security.yml lost workflow-analysis" unless security.fetch("jobs").key?("workflow-analysis")
events = security["on"] || security[true]
expected_branches = %w[development staging main]
expected_paths = [
  ".github/**",
  "scripts/check-branch-workflow-policy.rb",
  "scripts/gh-actions-policy.sh",
  "scripts/install-gitleaks-ci.sh",
  "scripts/scan-secrets.sh",
  ".gitleaks.toml"
]
%w[push pull_request].each do |event|
  config = events.fetch(event)
  abort "security.yml #{event} branches changed" unless config.fetch("branches") == expected_branches
  abort "security.yml #{event} paths changed" unless config.fetch("paths") == expected_paths
end
RUBY
}

workflow_core_checks() {
  ruby -rpsych <<'RUBY'
ci = Psych.safe_load_file(".github/workflows/ci.yml", aliases: false)
branch_flow = Psych.safe_load_file(".github/workflows/branch-flow.yml", aliases: false)
(ci.fetch("jobs").keys - ["cross-platform"]).each { |job| puts "ci\t#{job}" }
(branch_flow.fetch("jobs").keys - ["promotion-notice"]).each do |job|
  puts "branch-flow\t#{job}"
end
RUBY
}

self_test() {
  local core security ordinary staging_to_development development_to_staging
  local staging_to_main hotfix_to_main all_core wrong_workflow path tab derived_core
  tab=$'\t'
  SELF_TESTS=0
  SELF_TEST_FAILURES=0

  SELF_TESTS=$((SELF_TESTS + 1))
  if workflow_contract_valid; then
    printf 'ok %d - workflow names, jobs, routes, and security paths match the watcher\n' "$SELF_TESTS"
  else
    printf 'not ok %d - workflow/readiness contract drifted\n' "$SELF_TESTS" >&2
    SELF_TEST_FAILURES=$((SELF_TEST_FAILURES + 1))
  fi

  core=$(expected_checks development feature/tax 'crates/pos-domain/src/lib.rs')
  assert_has_check 'ordinary PR requires rust' "$core" "ci${tab}rust"
  assert_has_check 'ordinary PR requires guards' "$core" $'ci\tguards'
  assert_has_check 'ordinary PR requires web' "$core" $'ci\tweb'
  assert_has_check 'ordinary PR requires supply-chain' "$core" $'ci\tsupply-chain'
  assert_has_check 'ordinary PR requires protected paths' "$core" $'branch-flow\tprotected-paths'
  assert_has_check 'ordinary PR requires topology' "$core" $'branch-flow\ttopology'
  assert_lacks_check 'ordinary PR does not require security workflow' "$core" $'security\tworkflow-analysis'
  assert_lacks_check 'ordinary PR does not require a matrix' "$core" $'ci\tcross-platform (windows-latest)'
  assert_lacks_check 'ordinary PR does not require promotion notice' "$core" $'branch-flow\tpromotion-notice'
  derived_core=$(workflow_core_checks)
  assert_complete 'watcher includes every core job derived from the workflow files' "$derived_core" "$core"
  assert_complete 'watcher names no nonexistent core workflow job' "$core" "$derived_core"

  all_core="$core"$'\n'$'unrelated-workflow\tunrelated-check'
  assert_complete 'unrelated successful checks do not disturb a complete core' "$core" "$all_core"
  assert_incomplete 'one early check is not readiness' "$core" "ci${tab}rust"
  assert_incomplete 'a missing core check is refused' "$core" "$(printf '%s\n' "$core" | sed '/supply-chain/d')"
  wrong_workflow=$(printf '%s\n' "$core" | sed "s/^ci${tab}rust$/attacker${tab}rust/")
  assert_incomplete 'the right job name from the wrong workflow is refused' "$core" "$wrong_workflow"
  wrong_workflow=$(printf '%s\n' "$core" | sed "s/^ci${tab}rust$/ci${tab}rust (fake)/")
  assert_incomplete 'a check-name suffix cannot impersonate an exact job' "$core" "$wrong_workflow"
  assert_equal_value 'the canonical CI workflow path and event are accepted' 'ci' \
    "$(canonical_workflow_key '.github/workflows/ci.yml' pull_request ci)"
  assert_equal_value 'a same-name candidate workflow cannot impersonate CI' '' \
    "$(canonical_workflow_key '.github/workflows/candidate.yml' pull_request ci 2>/dev/null || true)"
  assert_equal_value 'a pull_request workflow cannot impersonate trusted branch-flow' '' \
    "$(canonical_workflow_key '.github/workflows/branch-flow.yml' pull_request branch-flow 2>/dev/null || true)"
  assert_equal_value 'a pending row counts only during registration' 'registered-only' "$({
    row_state_accepted registered PENDING && printf 'registered-'
    row_state_accepted successful PENDING || printf 'only'
  })"
  assert_equal_value 'only a successful required row is final evidence' 'success-only' "$({
    row_state_accepted successful SUCCESS && printf 'success-'
    row_state_accepted successful SKIPPED || printf 'only'
  })"

  security=$(expected_checks development feature/policy '.github/workflows/ci.yml')
  assert_has_check '.github changes require workflow analysis' "$security" $'security\tworkflow-analysis'
  assert_incomplete 'missing conditional workflow analysis is refused' "$security" "$core"

  for path in \
    scripts/check-branch-workflow-policy.rb \
    scripts/gh-actions-policy.sh \
    scripts/install-gitleaks-ci.sh \
    scripts/scan-secrets.sh \
    .gitleaks.toml; do
    security=$(expected_checks development feature/policy "$path")
    assert_has_check "$path triggers workflow analysis" "$security" $'security\tworkflow-analysis'
  done

  ordinary=$(expected_checks development feature/policy scripts/scan-secrets.sh.example)
  assert_lacks_check 'a similarly named path does not trigger security' "$ordinary" $'security\tworkflow-analysis'
  ordinary=$(expected_checks feature-base feature/policy '.github/workflows/ci.yml')
  assert_lacks_check 'security workflow is not expected on an unconfigured base' "$ordinary" $'security\tworkflow-analysis'
  security=$(expected_checks development feature/rename-out '.github/workflows/old.yml' 'docs/old-workflow.md')
  assert_has_check 'a renamed previous .github path still triggers security' "$security" $'security\tworkflow-analysis'

  development_to_staging=$(expected_checks staging development 'crates/pos-domain/src/lib.rs')
  assert_has_check 'development to staging requires Linux matrix' "$development_to_staging" $'ci\tcross-platform (ubuntu-22.04)'
  assert_has_check 'development to staging requires macOS matrix' "$development_to_staging" $'ci\tcross-platform (macos-latest)'
  assert_has_check 'development to staging requires Windows matrix' "$development_to_staging" $'ci\tcross-platform (windows-latest)'
  assert_has_check 'development to staging requires promotion notice' "$development_to_staging" $'branch-flow\tpromotion-notice'
  assert_incomplete 'missing Windows matrix result is refused' "$development_to_staging" "$(printf '%s\n' "$development_to_staging" | sed '/windows-latest/d')"

  staging_to_main=$(expected_checks main staging 'crates/pos-domain/src/lib.rs')
  assert_has_check 'staging to main requires the matrix' "$staging_to_main" $'ci\tcross-platform (windows-latest)'
  assert_has_check 'staging to main requires promotion notice' "$staging_to_main" $'branch-flow\tpromotion-notice'
  hotfix_to_main=$(expected_checks main hotfix/urgent 'crates/pos-domain/src/lib.rs')
  assert_has_check 'hotfix to main requires the matrix' "$hotfix_to_main" $'ci\tcross-platform (macos-latest)'
  assert_has_check 'hotfix to main requires promotion notice' "$hotfix_to_main" $'branch-flow\tpromotion-notice'

  staging_to_development=$(expected_checks development staging 'crates/pos-domain/src/lib.rs')
  assert_has_check 'staging to development requires promotion notice' "$staging_to_development" $'branch-flow\tpromotion-notice'
  assert_lacks_check 'staging to development does not require a matrix' "$staging_to_development" $'ci\tcross-platform (windows-latest)'

  security=$(expected_checks staging development '.github/labeler.yml')
  assert_has_check 'security promotion requires workflow analysis' "$security" $'security\tworkflow-analysis'
  assert_has_check 'security promotion also requires the matrix' "$security" $'ci\tcross-platform (ubuntu-22.04)'
  assert_has_check 'security promotion also requires promotion notice' "$security" $'branch-flow\tpromotion-notice'

  if [ "$SELF_TEST_FAILURES" -ne 0 ]; then
    printf '%d of %d readiness policy tests failed\n' "$SELF_TEST_FAILURES" "$SELF_TESTS" >&2
    return 1
  fi
  printf 'all %d readiness policy tests passed\n' "$SELF_TESTS"
}

pr_snapshot() {
  local pr=$1 response fields metadata metadata_fingerprint
  response=$(gh pr view "$pr" \
    --json number,baseRefName,baseRefOid,headRefName,headRefOid,changedFiles,state,url,title,body \
    --jq '([.number, .baseRefName, .baseRefOid, .headRefName, .headRefOid, .changedFiles, .state, .url] | @tsv), ([.title, (.body // "")] | tojson)') || return

  case "$response" in
    *$'\n'*) ;;
    *) return 1 ;;
  esac
  fields=${response%%$'\n'*}
  metadata=${response#*$'\n'}
  case "$metadata" in
    *$'\n'*) return 1 ;;
  esac
  metadata_fingerprint=$(printf '%s' "$metadata" | git hash-object --stdin) || return
  printf '%s\t%s\n' "$fields" "$metadata_fingerprint"
}

check_rows() {
  local pr=$1 repository=$2 mode=${3:-successful} raw status=0
  local row_name row_workflow row_event row_state link run_id run_record run_path run_event run_name canonical
  local cached_id cached_path cached_event cached_name
  local run_cache=''

  case "$mode" in registered|successful) ;; *) return 2 ;; esac

  raw=$(gh pr checks "$pr" --json name,workflow,event,state,link \
    --jq '.[] | [.name, .workflow, .event, .state, .link] | @tsv') || status=$?
  case "$status" in
    0|8) ;;
    *) return "$status" ;;
  esac

  while IFS=$'\t' read -r row_name row_workflow row_event row_state link; do
    [ -n "$row_name" ] || continue
    row_state_accepted "$mode" "$row_state" || continue
    if [[ "$link" =~ /actions/runs/([0-9]+)(/|$) ]]; then
      run_id=${BASH_REMATCH[1]}
    else
      continue
    fi

    run_record=''
    while IFS=$'\t' read -r cached_id cached_path cached_event cached_name; do
      if [ "$cached_id" = "$run_id" ]; then
        run_record=$(printf '%s\t%s\t%s' "$cached_path" "$cached_event" "$cached_name")
        break
      fi
    done <<< "$run_cache"

    if [ -z "$run_record" ]; then
      run_record=$(gh api "repos/$repository/actions/runs/$run_id" \
        --jq '[.path, .event, .name] | @tsv' 2>/dev/null) || continue
      if [ -n "$run_cache" ]; then
        run_cache+=$'\n'
      fi
      run_cache+=$(printf '%s\t%s' "$run_id" "$run_record")
    fi
    IFS=$'\t' read -r run_path run_event run_name <<< "$run_record"
    [ "$row_workflow" = "$run_name" ] && [ "$row_event" = "$run_event" ] || continue
    canonical=$(canonical_workflow_key "$run_path" "$run_event" "$run_name" 2>/dev/null) || continue
    printf '%s\t%s\n' "$canonical" "$row_name"
  done <<< "$raw"
}

assert_same_snapshot() {
  local pr=$1 expected=$2 stage=$3 actual
  actual=$(pr_snapshot "$pr") || {
    echo "unable to re-read $pr while $stage" >&2
    return 1
  }
  if [ "$actual" != "$expected" ]; then
    echo "PR snapshot changed while $stage; discard the old check evidence and run this command again" >&2
    return 1
  fi
}

main() {
  if [ "${1:-}" = '--self-test' ] && [ "$#" -eq 1 ]; then
    self_test
    return
  fi
  if [ "$#" -ne 1 ]; then
    usage
    return 2
  fi
  local command_name
  for command_name in gh git; do
    command -v "$command_name" >/dev/null 2>&1 || {
      echo "$command_name is required" >&2
      return 1
    }
  done

  local pr=$1 snapshot number base_ref base_sha head_ref head_sha changed_count state pr_url metadata_fingerprint
  local repository
  local changed_output record value api_count expected actual missing _attempt final_rows
  local -a changed_paths=()

  snapshot=$(pr_snapshot "$pr") || {
    echo "unable to read pull request: $pr" >&2
    return 1
  }
  if [ "$(printf '%s\n' "$snapshot" | wc -l | tr -d ' ')" != 1 ]; then
    echo "GitHub returned an ambiguous PR snapshot for $pr" >&2
    return 1
  fi
  IFS=$'\t' read -r number base_ref base_sha head_ref head_sha changed_count state pr_url metadata_fingerprint <<< "$snapshot"
  [[ "$number" =~ ^[0-9]+$ ]] || {
    echo "GitHub returned an invalid PR number: $number" >&2
    return 1
  }
  [[ "$base_sha" =~ ^[0-9a-f]{40}$ ]] || {
    echo "GitHub returned an invalid PR base SHA: $base_sha" >&2
    return 1
  }
  [[ "$head_sha" =~ ^[0-9a-f]{40}$ ]] || {
    echo "GitHub returned an invalid PR head SHA: $head_sha" >&2
    return 1
  }
  [[ "$changed_count" =~ ^[0-9]+$ ]] || {
    echo "GitHub returned an invalid changed-file count: $changed_count" >&2
    return 1
  }
  [[ "$metadata_fingerprint" =~ ^([0-9a-f]{40}|[0-9a-f]{64})$ ]] || {
    echo 'unable to fingerprint the PR title and body' >&2
    return 1
  }
  [ "$state" = 'OPEN' ] || {
    echo "pull request $pr_url is not open" >&2
    return 1
  }

  repository=$(gh repo view --json nameWithOwner --jq .nameWithOwner) || {
    echo 'unable to resolve the current GitHub repository' >&2
    return 1
  }
  [[ "$repository" =~ ^[^/[:space:]]+/[^/[:space:]]+$ ]] || {
    echo "GitHub returned an invalid repository name: $repository" >&2
    return 1
  }
  case "$pr_url" in
    */"$repository"/pull/"$number") ;;
    *)
      echo "pull request $pr_url does not belong to the current repository $repository" >&2
      return 1
      ;;
  esac

  # Include previous_filename so a rename out of a watched path cannot hide the
  # workflow that the old path triggered. Compare REST and GraphQL counts to
  # fail closed if GitHub's files endpoint truncates its 3,000-file result set.
  changed_output=$(gh api --paginate --slurp \
    "repos/$repository/pulls/$number/files?per_page=100" \
    --jq '(["COUNT", ([.[][]] | length | tostring)] | @tsv), (.[][] | ["PATH", .filename] | @tsv), (.[][] | select(.previous_filename != null) | ["PATH", .previous_filename] | @tsv)') || {
      echo "unable to enumerate every changed path for $pr_url" >&2
      return 1
    }

  api_count=''
  while IFS=$'\t' read -r record value; do
    case "$record" in
      COUNT)
        [ -z "$api_count" ] || {
          echo 'GitHub returned more than one changed-path count' >&2
          return 1
        }
        api_count=$value
        ;;
      PATH) changed_paths+=("$value") ;;
      *)
        echo 'GitHub returned an invalid changed-path record' >&2
        return 1
        ;;
    esac
  done <<< "$changed_output"

  [ -n "$api_count" ] && [[ "$api_count" =~ ^[0-9]+$ ]] || {
    echo 'GitHub did not return a valid changed-path count' >&2
    return 1
  }
  [ "$api_count" = "$changed_count" ] || {
    echo "changed-path enumeration is incomplete ($api_count of $changed_count); refusing partial readiness evidence" >&2
    return 1
  }
  assert_same_snapshot "$pr" "$snapshot" 'deriving required checks'

  expected=$(expected_checks "$base_ref" "$head_ref" "${changed_paths[@]}")
  printf 'Waiting for required checks on %s at %s:\n' "$pr_url" "$head_sha"
  while IFS= read -r value; do
    [ -n "$value" ] && printf '  - %s\n' "$(display_check "$value")"
  done <<< "$expected"

  missing=$expected
  for _attempt in $(seq 1 "$REGISTRATION_ATTEMPTS"); do
    actual=$(check_rows "$pr" "$repository" registered 2>/dev/null || true)
    missing=$(missing_checks "$expected" "$actual")
    [ -z "$missing" ] && break
    sleep "$REGISTRATION_DELAY_SECONDS"
  done
  if [ -n "$missing" ]; then
    echo "required checks did not all register for $pr_url@$head_sha:" >&2
    while IFS= read -r value; do
      [ -n "$value" ] && printf '  - %s\n' "$(display_check "$value")" >&2
    done <<< "$missing"
    return 1
  fi

  assert_same_snapshot "$pr" "$snapshot" 'waiting for check registration'
  gh pr checks "$pr" --watch --fail-fast
  assert_same_snapshot "$pr" "$snapshot" 'watching checks'

  if ! final_rows=$(check_rows "$pr" "$repository"); then
    echo "not every registered check succeeded for $pr_url@$head_sha" >&2
    return 1
  fi
  missing=$(missing_checks "$expected" "$final_rows")
  [ -z "$missing" ] || {
    echo 'the required check set changed before final verification; run this command again' >&2
    return 1
  }
  assert_same_snapshot "$pr" "$snapshot" 'performing final check verification'
  printf 'All required checks succeeded for %s at %s.\n' "$pr_url" "$head_sha"
}

main "$@"
