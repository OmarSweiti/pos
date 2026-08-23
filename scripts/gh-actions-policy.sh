#!/usr/bin/env bash
# Enable GitHub's full-SHA Actions policy only after every checked-in workflow
# has passed a local immutable-reference and repository-allowlist preflight.
#
# Exact `patterns_allowed` enforcement is intentionally not attempted here:
# GitHub documents that repository action patterns apply to private repositories
# only when the repository belongs to an enterprise. This private user-owned
# repository therefore retains `allowed_actions: all`; the full-SHA requirement
# is still server-enforced and this script rejects unknown action repositories.
set -euo pipefail
readonly MODE=${1:-}
if [ -n "${GH_ACTIONS_POLICY_ROOT:-}" ]; then
  if [ "$MODE" != "--check" ]; then
    echo "GH_ACTIONS_POLICY_ROOT is permitted only with the offline --check mode" >&2
    exit 2
  fi
  cd -- "$GH_ACTIONS_POLICY_ROOT"
else
  cd "$(dirname "$0")/.."
fi

readonly -a APPROVED_ACTIONS=(
  actions/checkout
  actions/download-artifact
  actions/labeler
  actions/setup-node
  actions/upload-artifact
  anchore/sbom-action
  EmbarkStudios/cargo-deny-action
  pnpm/action-setup
  raven-actions/actionlint
  Swatinem/rust-cache
  taiki-e/install-action
  tauri-apps/tauri-action
  dtolnay/rust-toolchain
  zizmorcore/zizmor-action
)

usage() {
  cat <<'TXT'
usage: ./scripts/gh-actions-policy.sh [--check|--dry-run|--self-test]

  no argument  preflight the workflows, then enable full-SHA enforcement
  --check      validate the current .github tree without contacting GitHub
  --dry-run    preflight and show the GitHub setting without changing it
  --self-test  exercise the preflight without contacting GitHub
TXT
}

preflight_dir() {
  local workflow_dir=$1
  command -v ruby >/dev/null 2>&1 || {
    echo "Ruby with its standard Psych YAML parser is required for the Actions policy preflight" >&2
    return 1
  }

  # YAML has several equivalent ways to spell a mapping key: quoted keys,
  # flow mappings, explicit `? key` mappings, and aliases can all mean `uses`.
  # Regex extraction cannot distinguish those safely. Inspect Psych's parsed
  # syntax tree instead, resolving scalar aliases before applying the policy.
  ruby -rpsych - "$workflow_dir" "${APPROVED_ACTIONS[@]}" <<'RUBY'
root = ARGV.shift
approved = ARGV.to_h { |repo| [repo, true] }
failures = 0
count = 0

def children(node)
  Array(node.respond_to?(:children) ? node.children : nil)
end

def collect_anchors(node, anchors, duplicate_anchors)
  if !node.is_a?(Psych::Nodes::Alias) && node.respond_to?(:anchor) && node.anchor
    duplicate_anchors << node.anchor if anchors.key?(node.anchor)
    anchors[node.anchor] = node
  end
  children(node).each { |child| collect_anchors(child, anchors, duplicate_anchors) }
end

def scalar_value(node, anchors)
  seen = {}
  while node.is_a?(Psych::Nodes::Alias)
    return nil if seen[node.anchor]

    seen[node.anchor] = true
    node = anchors[node.anchor]
    return nil unless node
  end
  node.is_a?(Psych::Nodes::Scalar) ? node.value : nil
end

def inspect_node(node, anchors, approved, path, counters)
  if node.is_a?(Psych::Nodes::Mapping)
    children(node).each_slice(2) do |key, value_node|
      if scalar_value(key, anchors) == "uses"
        counters[:count] += 1
        value = scalar_value(value_node, anchors)
        line = key.respond_to?(:start_line) ? key.start_line + 1 : "?"

        if !value.is_a?(String) || value.empty?
          warn format("  FAIL   %s:%s  uses must have a scalar action reference", path, line)
          counters[:failures] += 1
        elsif value.start_with?("./")
          warn format(
            "  FAIL   %s:%s  repository-local action %s is forbidden until its transitive code is trusted and pinned",
            path,
            line,
            value
          )
          counters[:failures] += 1
        else
          match = value.match(/\A([^\/@]+\/[^\/@]+)(\/[^@]+)?@([0-9a-f]{40})\z/)
          if !match
            warn format("  FAIL   %s:%s  %s is not pinned to a full commit SHA", path, line, value)
            counters[:failures] += 1
          elsif !approved[match[1]]
            warn format("  FAIL   %s:%s  %s is not in APPROVED_ACTIONS", path, line, match[1])
            counters[:failures] += 1
          else
            puts format("  ok     %-42s %s", match[1], match[3])
          end
        end
      end

      inspect_node(key, anchors, approved, path, counters)
      inspect_node(value_node, anchors, approved, path, counters)
    end
  else
    children(node).each { |child| inspect_node(child, anchors, approved, path, counters) }
  end
end

paths = Dir.glob(File.join(root, "**", "*.{yml,yaml}"))
if paths.empty?
  warn "no YAML files found under #{root}"
  exit 1
end

paths.each do |path|
  if File.symlink?(path)
    warn "  FAIL   #{path}  symbolic-link YAML is not accepted in .github"
    failures += 1
    next
  end

  begin
    document = Psych.parse_file(path)
    anchors = {}
    duplicate_anchors = []
    collect_anchors(document, anchors, duplicate_anchors)
    unless duplicate_anchors.empty?
      warn "  FAIL   #{path}  duplicate YAML anchors are ambiguous: #{duplicate_anchors.uniq.join(', ')}"
      failures += 1
      next
    end

    unresolved = []
    queue = [document]
    until queue.empty?
      node = queue.pop
      unresolved << node.anchor if node.is_a?(Psych::Nodes::Alias) && !anchors.key?(node.anchor)
      queue.concat(children(node))
    end
    unless unresolved.empty?
      warn "  FAIL   #{path}  unresolved YAML aliases: #{unresolved.uniq.join(', ')}"
      failures += 1
      next
    end

    counters = { count: 0, failures: 0 }
    inspect_node(document, anchors, approved, path, counters)
    count += counters[:count]
    failures += counters[:failures]
  rescue Psych::Exception, SystemCallError => error
    warn "  FAIL   #{path}  YAML could not be parsed: #{error.message}"
    failures += 1
  end
end

if count.zero?
  warn "no GitHub Action references found under #{root}"
  failures += 1
end
exit(failures.zero? ? 0 : 1)
RUBY
}

self_test() {
  local tmp pass=0 fail=0
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/pos-gh-actions-policy.XXXXXX")
  trap 'rm -rf "$tmp"' RETURN

  run_case() { # run_case <expected> <label> <uses-value>
    local expected=$1 label=$2 value=$3 got
    printf 'name: test\non: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: %s\n' "$value" > "$tmp/test.yml"
    if preflight_dir "$tmp" >/dev/null 2>&1; then got=0; else got=1; fi
    if [ "$got" -eq "$expected" ]; then
      printf '  ok    %s\n' "$label"
      pass=$((pass + 1))
    else
      printf '  FAIL  %s\n' "$label"
      fail=$((fail + 1))
    fi
  }

  echo "gh-actions-policy.sh — immutable reference preflight"
  run_case 0 "approved action at a full SHA" \
    "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"
  run_case 1 "repository-local action is refused until its code is transitively pinned" \
    "./.github/actions/example"
  run_case 1 "mutable tag" "actions/checkout@v7"
  run_case 1 "mutable branch" "dtolnay/rust-toolchain@master"
  run_case 1 "unknown repository even at a full SHA" \
    "unknown/example@0123456789abcdef0123456789abcdef01234567"

  printf '%s\n' \
    'name: test' \
    'on: push' \
    'jobs:' \
    '  test:' \
    '    runs-on: ubuntu-latest' \
    '    steps:' \
    '      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1' \
    '      - { uses: unknown/example@0123456789abcdef0123456789abcdef01234567 }' \
    > "$tmp/test.yml"
  if preflight_dir "$tmp" >/dev/null 2>&1; then
    printf '  FAIL  flow-style uses cannot bypass the verifier\n'
    fail=$((fail + 1))
  else
    printf '  ok    flow-style uses cannot bypass the verifier\n'
    pass=$((pass + 1))
  fi

  printf '%s\n' \
    'name: test' \
    'on: push' \
    'jobs:' \
    '  test:' \
    '    runs-on: ubuntu-latest' \
    '    steps:' \
    '      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1' \
    '      - ? uses' \
    '        : unknown/example@0123456789abcdef0123456789abcdef01234567' \
    > "$tmp/test.yml"
  if preflight_dir "$tmp" >/dev/null 2>&1; then
    printf '  FAIL  explicit mapping keys cannot bypass the verifier\n'
    fail=$((fail + 1))
  else
    printf '  ok    explicit mapping keys cannot bypass the verifier\n'
    pass=$((pass + 1))
  fi

  printf '%s\n' \
    'name: test' \
    'on: push' \
    'env:' \
    '  ACTION_KEY: &action_key uses' \
    'jobs:' \
    '  test:' \
    '    runs-on: ubuntu-latest' \
    '    steps:' \
    '      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1' \
    '      - *action_key: unknown/example@0123456789abcdef0123456789abcdef01234567' \
    > "$tmp/test.yml"
  if preflight_dir "$tmp" >/dev/null 2>&1; then
    printf '  FAIL  aliased uses keys cannot bypass the verifier\n'
    fail=$((fail + 1))
  else
    printf '  ok    aliased uses keys cannot bypass the verifier\n'
    pass=$((pass + 1))
  fi

  if preflight_dir .github >/dev/null; then
    printf '  ok    repository GitHub automation uses approved full-SHA actions\n'
    pass=$((pass + 1))
  else
    printf '  FAIL  repository GitHub automation uses approved full-SHA actions\n'
    fail=$((fail + 1))
  fi

  printf '\n%s passed, %s failed\n' "$pass" "$fail"
  [ "$fail" -eq 0 ]
}

case "$MODE" in
  --check)
    echo "GitHub Actions policy preflight"
    preflight_dir .github
    exit $?
    ;;
  --self-test)
    self_test
    exit $?
    ;;
  --dry-run|'') ;;
  -h|--help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

echo "GitHub Actions policy preflight"
preflight_dir .github

repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner) || {
  echo "gh is not authenticated. Run: gh auth login" >&2
  exit 1
}
current=$(gh api "repos/$repo/actions/permissions")
allowed=$(printf '%s' "$current" | jq -r .allowed_actions)
enabled=$(printf '%s' "$current" | jq -r .enabled)
sha_required=$(printf '%s' "$current" | jq -r .sha_pinning_required)
default_branch=$(gh repo view --json defaultBranchRef --jq .defaultBranchRef.name)
remote_head=$(gh api "repos/$repo/commits/$default_branch" --jq .sha)
local_head=$(git rev-parse HEAD)
relevant_status=$(git status --porcelain -- .github scripts/gh-actions-policy.sh)
ready_to_apply=true

if [ -n "$relevant_status" ]; then
  ready_to_apply=false
fi
if [ "$local_head" != "$remote_head" ]; then
  ready_to_apply=false
fi

echo
echo "repository: $repo"
echo "current: enabled=$enabled allowed_actions=$allowed sha_pinning_required=$sha_required"
echo "target:  enabled=true allowed_actions=$allowed sha_pinning_required=true"
echo "note: exact selected-action patterns are unavailable for this private, non-enterprise repository"
echo "default branch: $default_branch"
echo "local HEAD:     $local_head"
echo "remote HEAD:    $remote_head"
if [ -n "$relevant_status" ]; then
  echo "pre-apply status: REFUSE — GitHub automation/policy files have uncommitted changes"
  printf '%s\n' "$relevant_status"
elif [ "$local_head" != "$remote_head" ]; then
  echo "pre-apply status: REFUSE — local HEAD is not the live default-branch commit"
else
  echo "pre-apply status: ready"
fi

if [ "$MODE" = "--dry-run" ]; then
  echo "dry run — no GitHub setting changed"
  exit 0
fi

if [ "$ready_to_apply" != true ]; then
  echo "refusing to enable server enforcement from policy that is not the clean, live default branch" >&2
  echo "merge the pinned workflows, update $default_branch, and run this script again from that exact clean commit" >&2
  exit 1
fi

gh api --method PUT "repos/$repo/actions/permissions" \
  -F enabled=true \
  -f allowed_actions="$allowed" \
  -F sha_pinning_required=true >/dev/null

actual=$(gh api "repos/$repo/actions/permissions")
printf '%s' "$actual" | jq -e \
  --arg allowed "$allowed" \
  '.enabled == true and .allowed_actions == $allowed and .sha_pinning_required == true' \
  >/dev/null || {
    echo "GitHub accepted the request but the resulting policy does not match the target" >&2
    exit 1
  }
echo "full-SHA enforcement enabled; existing action-allowance mode preserved"
