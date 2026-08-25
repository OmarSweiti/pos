#!/usr/bin/env ruby
# frozen_string_literal: true

# Fail closed if branch-flow.yml stops being a read-only pull_request_target
# workflow that treats the exact PR head as data and executes only policy from
# the commit that supplied the trusted workflow definition. Also prevent a
# green PR from weakening the workflow/policy blobs that a future trusted
# workflow revision would execute.

require "psych"
require "json"
require "fileutils"
require "find"
require "open3"
require "tmpdir"

class PolicyViolation < StandardError; end

BASE_SHA = "${{ github.event.pull_request.base.sha }}"
HEAD_SHA = "${{ github.event.pull_request.head.sha }}"
WORKFLOW_SHA = "${{ github.workflow_sha }}"
CHECKOUT = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"
LABELER_ACTION = "actions/labeler@bf12e9b00b37c5c0ca2b87b79b2daf7891dbda13"

EXPECTED_TOP_LEVEL_KEYS = %w[name on permissions concurrency jobs].freeze
EXPECTED_JOBS = %w[protected-paths topology promotion-notice].freeze
EXPECTED_EVENT_TYPES = %w[opened edited reopened synchronize].freeze
STATIC_POLICY_PATHS = %w[
  .gitattributes
  .nvmrc
  .agents/test-skills.py
  .claude/hooks/docs-links-on-write.py
  .claude/hooks/docs-links-on-write.sh
  .claude/hooks/protect-immutable.py
  .claude/hooks/run-python-hook.mjs
  .claude/hooks/test-docs-links.sh
  .claude/hooks/test-protect-immutable.sh
  .claude/hooks/test-settings.py
  .claude/hooks/validate-settings.py
  .claude/settings.json
  .codex/config.toml
  .codex/hooks.json
  .codex/hooks/docs-links-on-patch.py
  .codex/hooks/protect-immutable.py
  .codex/hooks/test-hooks.sh
  .codex/rules/safety.rules
  .codex/test-policy.py
  .github/dependabot.yml
  .github/labeler.yml
  .gitleaks.toml
  deny.toml
  js-license-policy.json
  ruff.toml
  justfile
  scripts/check-automation-attribution.py
  scripts/check-branch-workflow-policy.rb
  scripts/check-doc-links.py
  scripts/check-doc-links.sh
  scripts/check-node-version.py
  scripts/check-domain-acyclic.py
  scripts/check-domain-purity.py
  scripts/check-justfile-policy.py
  scripts/check-js-licenses.py
  scripts/check-logical-css.sh
  scripts/check-prop-test-names.py
  scripts/check-protected-paths.sh
  scripts/check-staged-policy.py
  scripts/check-workspace-lints.py
  scripts/check-web-build-coverage.py
  scripts/gh-bootstrap.sh
  scripts/gh-project.sh
  scripts/gh-actions-policy.sh
  scripts/install-gitleaks-ci.sh
  scripts/install-script-linters-ci.sh
  scripts/lint-scripts.sh
  scripts/pr-type-label.sh
  scripts/report-test-coverage.sh
  scripts/run-python.sh
  scripts/rust_lexer.py
  scripts/scan-secrets.sh
  scripts/test-gh-setup.sh
  scripts/validate-branch-flow.sh
  scripts/validate-change-title.sh
  scripts/verify-pg-migrations.py
  scripts/verify-schema.py
  scripts/watch-pr-checks.sh
].freeze
STRUCTURAL_POLICY_PATHS = %w[
  .nvmrc
  Cargo.toml
  apps/backoffice/package.json
  apps/terminal/package.json
  biome.json
  package.json
  pnpm-workspace.yaml
  rust-toolchain.toml
].freeze
APPROVED_BIOME_EXCLUSIONS = %w[
  !**/dist
  !**/src-tauri
  !**/node_modules
  !**/public/**/*.svg
].freeze
WORKFLOW_SUFFIXES = %w[.yml .yaml].freeze
LOCAL_ACTIONS_PATH = ".github/actions"

EXPECTED_STEPS = {
  "protected-paths" => [
    "Check out policy from the exact trusted workflow revision",
    "Materialize the verified untrusted head as data only",
    "The next workflow retains this trusted-workflow boundary",
    "No PR may edit a source plan or an existing migration",
    "PR commits contain no assistant-attribution trailers",
    "GitHub automation uses only approved full-SHA actions"
  ],
  "topology" => [
    "Check out policy from the exact trusted workflow revision",
    "A PR must flow feature → development → staging → main",
    "PR title is the squash commit message, so it obeys conventions §8",
    "PR title and body contain no assistant attribution"
  ],
  "promotion-notice" => [
    "Warn that this PR must use a merge commit"
  ]
}.freeze

EXPECTED_RUN = {
  "Materialize the verified untrusted head as data only" => <<~'SH'.rstrip,
    set -euo pipefail
    git cat-file -e "$BASE_SHA^{commit}" || {
      echo "::error::event base SHA is not present in the trusted repository clone"
      exit 1
    }
    auth_header=$(printf 'x-access-token:%s' "$GH_TOKEN" | base64 | tr -d '\n')
    git -c "http.https://github.com/.extraheader=AUTHORIZATION: basic $auth_header" \
      fetch --no-tags --no-recurse-submodules origin \
      "+refs/pull/$PR_NUMBER/head:refs/remotes/pull/$PR_NUMBER/head"
    unset auth_header
    fetched=$(git rev-parse "refs/remotes/pull/$PR_NUMBER/head^{commit}")
    [ "$fetched" = "$HEAD_SHA" ] || {
      echo "::error::fetched PR head $fetched does not match event head $HEAD_SHA"
      exit 1
    }
    candidate_root="$RUNNER_TEMP/candidate"
    [ ! -e "$candidate_root" ] || {
      echo "::error::candidate worktree path already exists"
      exit 1
    }
    git -c core.hooksPath=/dev/null -c submodule.recurse=false \
      worktree add --detach "$candidate_root" "$HEAD_SHA"
    [ "$(git -C "$candidate_root" rev-parse HEAD)" = "$HEAD_SHA" ] || {
      echo "::error::candidate worktree is not the verified event head"
      exit 1
    }
  SH
  "The next workflow retains this trusted-workflow boundary" => <<~'SH'.rstrip,
    set -euo pipefail
    policy="$GITHUB_WORKSPACE/scripts/check-branch-workflow-policy.rb"
    [ -f "$policy" ] || {
      echo "::error::trusted workflow revision lacks scripts/check-branch-workflow-policy.rb"
      exit 1
    }
    candidate_root="$RUNNER_TEMP/candidate"
    ruby "$policy" \
      --candidate-root "$candidate_root" \
      --trusted-revision "$TRUSTED_POLICY_SHA" \
      --candidate-revision "$HEAD_SHA"
  SH
  "No PR may edit a source plan or an existing migration" => <<~'SH'.rstrip,
    set -euo pipefail
    "$GITHUB_WORKSPACE/scripts/check-protected-paths.sh" \
      "$BASE_SHA" "$HEAD_SHA"
  SH
  "PR commits contain no assistant-attribution trailers" => <<~'SH'.rstrip,
    set -euo pipefail
    verifier="$GITHUB_WORKSPACE/scripts/check-automation-attribution.py"
    commits="$RUNNER_TEMP/pr-commits.txt"
    git rev-list "$BASE_SHA..$HEAD_SHA" > "$commits"
    while IFS= read -r commit; do
      [ -n "$commit" ] || continue
      "$verifier" --git-commit "$commit"
    done < "$commits"
  SH
  "GitHub automation uses only approved full-SHA actions" => <<~'SH'.rstrip,
    set -euo pipefail
    candidate_root="$RUNNER_TEMP/candidate"
    GH_ACTIONS_POLICY_ROOT="$candidate_root" \
      "$GITHUB_WORKSPACE/scripts/gh-actions-policy.sh" --check
  SH
  "A PR must flow feature → development → staging → main" => <<~'SH'.rstrip,
    set -euo pipefail
    "$GITHUB_WORKSPACE/scripts/validate-branch-flow.sh" \
      "$HEAD_REF" "$BASE_REF" "$HEAD_REPO" "$BASE_REPO"
  SH
  "PR title is the squash commit message, so it obeys conventions §8" => <<~'SH'.rstrip,
    set -euo pipefail
    case "$HEAD_REF" in
      development|staging|main|hotfix/*)
        echo "promotion, back-merge, or hotfix PR — merged with a merge commit, so the title is free text. Skipping."
        exit 0 ;;
    esac
    "$GITHUB_WORKSPACE/scripts/validate-change-title.sh" "$TITLE"
  SH
  "PR title and body contain no assistant attribution" => <<~'SH'.rstrip,
    set -euo pipefail
    message_file="$RUNNER_TEMP/pr-title-and-body.txt"
    printf '%s\n%s' "$TITLE" "$PR_BODY" > "$message_file"
    "$GITHUB_WORKSPACE/scripts/check-automation-attribution.py" \
      --message-file "$message_file"
  SH
  "Warn that this PR must use a merge commit" => <<~'SH'.rstrip
    echo "::warning::Non-squash PR ($HEAD_REF → $BASE_REF) — use 'Create a merge commit'. A squash here permanently forks the branches."
  SH
}.freeze

EXPECTED_ENV = {
  "Materialize the verified untrusted head as data only" => {
    "BASE_SHA" => BASE_SHA,
    "HEAD_SHA" => HEAD_SHA,
    "PR_NUMBER" => "${{ github.event.pull_request.number }}",
    "GH_TOKEN" => "${{ github.token }}"
  },
  "The next workflow retains this trusted-workflow boundary" => {
    "TRUSTED_POLICY_SHA" => WORKFLOW_SHA,
    "HEAD_SHA" => HEAD_SHA
  },
  "No PR may edit a source plan or an existing migration" => {
    "BASE_SHA" => BASE_SHA,
    "HEAD_SHA" => HEAD_SHA
  },
  "PR commits contain no assistant-attribution trailers" => {
    "BASE_SHA" => BASE_SHA,
    "HEAD_SHA" => HEAD_SHA
  },
  "A PR must flow feature → development → staging → main" => {
    "HEAD_REF" => "${{ github.head_ref }}",
    "BASE_REF" => "${{ github.base_ref }}",
    "HEAD_REPO" => "${{ github.event.pull_request.head.repo.full_name }}",
    "BASE_REPO" => "${{ github.event.pull_request.base.repo.full_name }}"
  },
  "PR title is the squash commit message, so it obeys conventions §8" => {
    "HEAD_REF" => "${{ github.head_ref }}",
    "TITLE" => "${{ github.event.pull_request.title }}"
  },
  "PR title and body contain no assistant attribution" => {
    "TITLE" => "${{ github.event.pull_request.title }}",
    "PR_BODY" => "${{ github.event.pull_request.body }}"
  },
  "Warn that this PR must use a merge commit" => {
    "HEAD_REF" => "${{ github.head_ref }}",
    "BASE_REF" => "${{ github.base_ref }}"
  }
}.freeze

EXPECTED_WORKING_DIRECTORY = {}.freeze

def children(node)
  Array(node.respond_to?(:children) ? node.children : nil)
end

def walk(node, &block)
  yield node
  children(node).each { |child| walk(child, &block) }
end

def scalar(node, context)
  unless node.is_a?(Psych::Nodes::Scalar)
    raise PolicyViolation, "#{context} must be a scalar"
  end

  node.value
end

def mapping(node, context)
  unless node.is_a?(Psych::Nodes::Mapping)
    raise PolicyViolation, "#{context} must be a mapping"
  end

  result = {}
  children(node).each_slice(2) do |key_node, value_node|
    key = scalar(key_node, "#{context} key")
    raise PolicyViolation, "#{context} contains duplicate key #{key.inspect}" if result.key?(key)

    result[key] = value_node
  end
  result
end

def sequence(node, context)
  unless node.is_a?(Psych::Nodes::Sequence)
    raise PolicyViolation, "#{context} must be a sequence"
  end

  children(node)
end

def scalar_mapping(node, context)
  mapping(node, context).to_h do |key, value_node|
    [key, scalar(value_node, "#{context}.#{key}")]
  end
end

def require_exact_keys(actual, expected, context)
  return if actual.keys.sort == expected.sort

  raise PolicyViolation,
        "#{context} keys must be exactly #{expected.sort.inspect}; got #{actual.keys.sort.inspect}"
end

def require_scalar(map, key, expected, context)
  actual = scalar(map.fetch(key) { raise PolicyViolation, "#{context}.#{key} is required" }, "#{context}.#{key}")
  return if actual == expected

  raise PolicyViolation, "#{context}.#{key} must be #{expected.inspect}; got #{actual.inspect}"
end

def validate_checkout(step, context)
  require_scalar(step, "uses", CHECKOUT, context)
  require_exact_keys(step, %w[name uses with], context)
  inputs = scalar_mapping(step.fetch("with"), "#{context}.with")
  require_exact_keys(inputs, %w[fetch-depth persist-credentials ref], "#{context}.with")

  expected = {
    "ref" => WORKFLOW_SHA,
    "fetch-depth" => "0",
    "persist-credentials" => "false"
  }

  return if inputs == expected

  raise PolicyViolation,
        "#{context} must check out only the exact workflow revision with inputs #{expected.inspect}; got #{inputs.inspect}"
end

def validate_run_step(step, name, context)
  expected_keys = %w[name run]
  expected_keys << "env" if EXPECTED_ENV.key?(name)
  expected_keys << "working-directory" if EXPECTED_WORKING_DIRECTORY.key?(name)
  require_exact_keys(step, expected_keys, context)

  run = scalar(step.fetch("run"), "#{context}.run").rstrip
  expected_run = EXPECTED_RUN.fetch(name)
  unless run == expected_run
    raise PolicyViolation, "#{context}.run changed; executable policy wiring must match the reviewed contract"
  end
  if run.include?("${{")
    raise PolicyViolation, "#{context}.run must not interpolate GitHub expressions into shell source"
  end

  if EXPECTED_ENV.key?(name)
    actual_env = scalar_mapping(step.fetch("env"), "#{context}.env")
    expected_env = EXPECTED_ENV.fetch(name)
    unless actual_env == expected_env
      raise PolicyViolation, "#{context}.env must be #{expected_env.inspect}; got #{actual_env.inspect}"
    end
  end

  if EXPECTED_WORKING_DIRECTORY.key?(name)
    require_scalar(step, "working-directory", EXPECTED_WORKING_DIRECTORY.fetch(name), context)
  end
end

def validate_job(job_name, job_node)
  context = "jobs.#{job_name}"
  job = mapping(job_node, context)
  expected_keys = %w[runs-on timeout-minutes steps]
  expected_keys << "if" if job_name == "promotion-notice"
  require_exact_keys(job, expected_keys, context)
  require_scalar(job, "runs-on", "ubuntu-latest", context)
  require_scalar(job, "timeout-minutes", job_name == "promotion-notice" ? "5" : "10", context)
  if job_name == "promotion-notice"
    require_scalar(
      job,
      "if",
      "github.head_ref == 'development' || github.head_ref == 'staging' || github.head_ref == 'main' || startsWith(github.head_ref, 'hotfix/')",
      context
    )
  end

  steps = sequence(job.fetch("steps"), "#{context}.steps")
  expected_names = EXPECTED_STEPS.fetch(job_name)
  if steps.length != expected_names.length
    raise PolicyViolation, "#{context} must contain exactly #{expected_names.length} reviewed steps"
  end

  steps.each_with_index do |step_node, index|
    step_context = "#{context}.steps[#{index}]"
    step = mapping(step_node, step_context)
    name = scalar(step.fetch("name") { raise PolicyViolation, "#{step_context}.name is required" }, "#{step_context}.name")
    expected_name = expected_names.fetch(index)
    unless name == expected_name
      raise PolicyViolation, "#{step_context}.name must be #{expected_name.inspect}; got #{name.inspect}"
    end

    if name == "Check out policy from the exact trusted workflow revision"
      validate_checkout(step, step_context)
    else
      validate_run_step(step, name, step_context)
    end
  end
end

def validate_file(path)
  raise PolicyViolation, "workflow is missing: #{path}" unless File.file?(path)
  raise PolicyViolation, "workflow must not be a symbolic link: #{path}" if File.symlink?(path)

  document = Psych.parse_file(path)
  raise PolicyViolation, "workflow is empty" unless document&.root

  walk(document) do |node|
    raise PolicyViolation, "YAML aliases are forbidden in this security boundary" if node.is_a?(Psych::Nodes::Alias)

    next unless node.is_a?(Psych::Nodes::Scalar)

    value = node.value
    if value.match?(/\$\{\{[^}]*\bsecrets\s*\./im)
      raise PolicyViolation, "branch-flow must not reference secrets"
    end
    if value.match?(/\A(?:actions\/cache|[^\s@]*cache[^\s@]*)@/i)
      raise PolicyViolation, "branch-flow must not use cache actions"
    end
  end

  root = mapping(document.root, "workflow")
  require_exact_keys(root, EXPECTED_TOP_LEVEL_KEYS, "workflow")
  require_scalar(root, "name", "branch-flow", "workflow")

  events = mapping(root.fetch("on"), "on")
  require_exact_keys(events, ["pull_request_target"], "on")
  target = mapping(events.fetch("pull_request_target"), "on.pull_request_target")
  require_exact_keys(target, ["types"], "on.pull_request_target")
  types = sequence(target.fetch("types"), "on.pull_request_target.types").map.with_index do |node, index|
    scalar(node, "on.pull_request_target.types[#{index}]")
  end
  unless types == EXPECTED_EVENT_TYPES && types.uniq.length == types.length
    raise PolicyViolation,
          "pull_request_target types must be exactly #{EXPECTED_EVENT_TYPES.inspect}; got #{types.inspect}"
  end

  permissions = scalar_mapping(root.fetch("permissions"), "permissions")
  expected_permissions = { "contents" => "read" }
  unless permissions == expected_permissions
    raise PolicyViolation, "permissions must be exactly #{expected_permissions.inspect}; got #{permissions.inspect}"
  end

  concurrency = scalar_mapping(root.fetch("concurrency"), "concurrency")
  expected_concurrency = {
    "group" => "branch-flow-${{ github.event.pull_request.number }}",
    "cancel-in-progress" => "true"
  }
  unless concurrency == expected_concurrency
    raise PolicyViolation, "concurrency must be exactly #{expected_concurrency.inspect}; got #{concurrency.inspect}"
  end

  jobs = mapping(root.fetch("jobs"), "jobs")
  require_exact_keys(jobs, EXPECTED_JOBS, "jobs")
  EXPECTED_JOBS.each { |job_name| validate_job(job_name, jobs.fetch(job_name)) }
end

def validate_labeler_file(path)
  raise PolicyViolation, "labeler workflow is missing: #{path}" unless File.file?(path)
  raise PolicyViolation, "labeler workflow must not be a symbolic link: #{path}" if File.symlink?(path)

  document = Psych.parse_file(path)
  raise PolicyViolation, "labeler workflow is empty" unless document&.root
  walk(document) do |node|
    raise PolicyViolation, "YAML aliases are forbidden in the labeler security boundary" if node.is_a?(Psych::Nodes::Alias)
  end

  root = mapping(document.root, "labeler workflow")
  require_exact_keys(root, EXPECTED_TOP_LEVEL_KEYS, "labeler workflow")
  require_scalar(root, "name", "labeler", "labeler workflow")

  events = mapping(root.fetch("on"), "labeler on")
  require_exact_keys(events, ["pull_request_target"], "labeler on")
  target = mapping(events.fetch("pull_request_target"), "labeler on.pull_request_target")
  require_exact_keys(target, ["types"], "labeler on.pull_request_target")
  types = sequence(target.fetch("types"), "labeler on.pull_request_target.types").map.with_index do |node, index|
    scalar(node, "labeler on.pull_request_target.types[#{index}]")
  end
  expected_types = %w[opened synchronize reopened edited]
  unless types == expected_types && types.uniq.length == types.length
    raise PolicyViolation,
          "labeler pull_request_target types must be exactly #{expected_types.inspect}; got #{types.inspect}"
  end

  permissions = scalar_mapping(root.fetch("permissions"), "labeler permissions")
  expected_permissions = { "contents" => "read", "pull-requests" => "write" }
  unless permissions == expected_permissions
    raise PolicyViolation,
          "labeler permissions must be exactly #{expected_permissions.inspect}; got #{permissions.inspect}"
  end

  concurrency = scalar_mapping(root.fetch("concurrency"), "labeler concurrency")
  expected_concurrency = {
    "group" => "labeler-${{ github.event.pull_request.number }}",
    "cancel-in-progress" => "true"
  }
  unless concurrency == expected_concurrency
    raise PolicyViolation,
          "labeler concurrency must be exactly #{expected_concurrency.inspect}; got #{concurrency.inspect}"
  end

  jobs = mapping(root.fetch("jobs"), "labeler jobs")
  require_exact_keys(jobs, ["label"], "labeler jobs")
  job = mapping(jobs.fetch("label"), "labeler jobs.label")
  require_exact_keys(job, %w[runs-on timeout-minutes steps], "labeler jobs.label")
  require_scalar(job, "runs-on", "ubuntu-latest", "labeler jobs.label")
  require_scalar(job, "timeout-minutes", "10", "labeler jobs.label")
  steps = sequence(job.fetch("steps"), "labeler jobs.label.steps")
  unless steps.length == 4
    raise PolicyViolation, "labeler job must contain exactly four reviewed steps"
  end

  label_step = mapping(steps.fetch(0), "labeler jobs.label.steps[0]")
  require_exact_keys(label_step, %w[uses with], "labeler jobs.label.steps[0]")
  require_scalar(label_step, "uses", LABELER_ACTION, "labeler jobs.label.steps[0]")
  label_inputs = scalar_mapping(label_step.fetch("with"), "labeler jobs.label.steps[0].with")
  expected_label_inputs = {
    "configuration-path" => ".github/labeler.yml",
    "sync-labels" => "true"
  }
  unless label_inputs == expected_label_inputs
    raise PolicyViolation, "labeler action inputs changed from #{expected_label_inputs.inspect}"
  end

  checkout_step = mapping(steps.fetch(1), "labeler jobs.label.steps[1]")
  require_exact_keys(checkout_step, %w[uses with], "labeler jobs.label.steps[1]")
  require_scalar(checkout_step, "uses", CHECKOUT, "labeler jobs.label.steps[1]")
  checkout_inputs = scalar_mapping(checkout_step.fetch("with"), "labeler jobs.label.steps[1].with")
  expected_checkout_inputs = {
    "ref" => WORKFLOW_SHA,
    "persist-credentials" => "false"
  }
  unless checkout_inputs == expected_checkout_inputs
    raise PolicyViolation,
          "labeler must check out only the exact workflow revision with #{expected_checkout_inputs.inspect}; " \
          "got #{checkout_inputs.inspect}"
  end

  run_steps = [
    [steps.fetch(2), "Normalize a Dependabot title to the repository convention", true],
    [steps.fetch(3), "The type label comes from the PR title", false]
  ]
  run_steps.each_with_index do |(node, expected_name, conditional), offset|
    context = "labeler jobs.label.steps[#{offset + 2}]"
    step = mapping(node, context)
    keys = %w[name env run]
    keys << "if" if conditional
    require_exact_keys(step, keys, context)
    require_scalar(step, "name", expected_name, context)
    if conditional
      require_scalar(
        step,
        "if",
        "github.event.pull_request.user.login == 'dependabot[bot]'",
        context
      )
    end
    env = scalar_mapping(step.fetch("env"), "#{context}.env")
    expected_env = {
      "GH_TOKEN" => "${{ secrets.GITHUB_TOKEN }}",
      "GH_REPO" => "${{ github.repository }}",
      "TITLE" => "${{ github.event.pull_request.title }}",
      "NUMBER" => "${{ github.event.pull_request.number }}"
    }
    unless env == expected_env
      raise PolicyViolation, "#{context}.env must be exactly #{expected_env.inspect}; got #{env.inspect}"
    end
    run = scalar(step.fetch("run"), "#{context}.run")
    if run.include?("${{") || run.include?("CANDIDATE_ROOT")
      raise PolicyViolation, "#{context}.run must treat event fields as env data and never use candidate code"
    end
  end
end

def reject_symlink_components(root, relative, context)
  current = root
  relative.split("/").each do |component|
    current = File.join(current, component)
    raise PolicyViolation, "#{context} contains symbolic-link component #{component.inspect}" if File.symlink?(current)
  end
end

def regular_policy_file(root, relative)
  reject_symlink_components(root, relative, "structural policy path #{relative}")
  path = File.join(root, relative)
  raise PolicyViolation, "structural policy file is missing: #{relative}" unless File.file?(path)
  raise PolicyViolation, "structural policy file must not be a symbolic link: #{relative}" if File.symlink?(path)

  path
end

def json_policy_file(root, relative)
  parsed = JSON.parse(File.read(regular_policy_file(root, relative)))
  unless parsed.is_a?(Hash)
    raise PolicyViolation, "#{relative} must contain a top-level JSON object"
  end

  parsed
rescue JSON::ParserError => error
  raise PolicyViolation, "#{relative} is invalid JSON: #{error.message}"
end

def yaml_policy_file(root, relative)
  path = regular_policy_file(root, relative)
  document = Psych.parse_file(path)
  raise PolicyViolation, "#{relative} is empty" unless document&.root
  walk(document) do |node|
    if node.is_a?(Psych::Nodes::Alias)
      raise PolicyViolation, "#{relative} must not use YAML aliases"
    end
  end
  parsed = Psych.safe_load_file(path, aliases: false)
  unless parsed.is_a?(Hash)
    raise PolicyViolation, "#{relative} must contain a top-level YAML mapping"
  end

  parsed
rescue Psych::Exception => error
  raise PolicyViolation, "#{relative} is invalid YAML: #{error.message}"
end

def toml_string(root, relative, section, key)
  path = regular_policy_file(root, relative)
  current_section = nil
  matches = []
  File.foreach(path).with_index(1) do |line, line_number|
    if (heading = line.match(/^\s*\[([^\]]+)\]\s*(?:#.*)?$/))
      current_section = heading[1]
      next
    end
    next unless current_section == section
    next unless line.match?(/^\s*#{Regexp.escape(key)}\s*=/)

    value = line.match(/^\s*#{Regexp.escape(key)}\s*=\s*"([^"]+)"\s*(?:#.*)?$/)
    unless value
      raise PolicyViolation,
            "#{relative} [#{section}].#{key} must be one plain quoted string (line #{line_number})"
    end
    matches << value[1]
  end
  unless matches.length == 1
    raise PolicyViolation,
          "#{relative} must declare [#{section}].#{key} exactly once; found #{matches.length}"
  end

  matches.first
end

def validate_repository_configuration(root)
  STRUCTURAL_POLICY_PATHS.each { |relative| regular_policy_file(root, relative) }

  node_version = File.read(regular_policy_file(root, ".nvmrc")).strip
  unless node_version.match?(/\A(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\z/)
    raise PolicyViolation, ".nvmrc must contain one exact Node semantic version"
  end

  package = json_policy_file(root, "package.json")
  package_manager = package["packageManager"]
  unless package_manager.is_a?(String) &&
         package_manager.match?(/\Apnpm@(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\z/)
    raise PolicyViolation, "package.json packageManager must pin one exact pnpm version"
  end
  unless package.dig("engines", "node") == node_version
    raise PolicyViolation, "package.json engines.node must exactly equal .nvmrc (#{node_version})"
  end
  unless package.dig("scripts", "lint") == "biome ci --error-on-warnings ."
    raise PolicyViolation, "package.json scripts.lint must keep Biome fail-closed on warnings"
  end
  unless package.dig("scripts", "build:web") == "just build-web"
    raise PolicyViolation,
          "package.json scripts.build:web must route through the checked just build-web gate"
  end

  pnpm = yaml_policy_file(root, "pnpm-workspace.yaml")
  unless pnpm["nodeVersion"].to_s == node_version
    raise PolicyViolation, "pnpm-workspace.yaml nodeVersion must exactly equal .nvmrc (#{node_version})"
  end
  unless pnpm["engineStrict"] == true
    raise PolicyViolation, "pnpm-workspace.yaml engineStrict must remain true"
  end
  expected_workspace_patterns = ["apps/terminal", "apps/backoffice", "packages/*"]
  unless pnpm["packages"] == expected_workspace_patterns
    raise PolicyViolation,
          "pnpm-workspace.yaml packages must remain #{expected_workspace_patterns.inspect}"
  end
  expected_build_allowlist = {
    "esbuild" => true,
    "@tailwindcss/oxide" => true
  }
  unless pnpm["allowBuilds"] == expected_build_allowlist
    raise PolicyViolation,
          "pnpm-workspace.yaml allowBuilds must remain the reviewed #{expected_build_allowlist.inspect}"
  end
  types_node = pnpm.dig("overrides", "@types/node")
  unless types_node.is_a?(String) &&
         types_node.match?(/\A(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\z/) &&
         types_node.split(".").first == node_version.split(".").first
    raise PolicyViolation,
          "pnpm-workspace.yaml @types/node override must be an exact release on Node #{node_version.split('.').first}"
  end
  %w[apps/backoffice/package.json apps/terminal/package.json].each do |relative|
    manifest = json_policy_file(root, relative)
    unless manifest.dig("devDependencies", "@types/node") == types_node
      raise PolicyViolation,
            "#{relative} devDependencies.@types/node must equal the workspace override #{types_node.inspect}"
    end
  end

  biome = json_policy_file(root, "biome.json")
  unless biome.dig("linter", "enabled") == true &&
         biome.dig("linter", "rules", "preset") == "recommended"
    raise PolicyViolation, "biome.json must keep the recommended linter enabled"
  end
  includes = biome.dig("files", "includes")
  unless includes.is_a?(Array) && includes.all? { |entry| entry.is_a?(String) }
    raise PolicyViolation, "biome.json files.includes must be a string array"
  end
  %w[apps/** packages/**].each do |required|
    unless includes.include?(required)
      raise PolicyViolation, "biome.json files.includes must cover #{required}"
    end
  end
  unreviewed_exclusions = includes.grep(/\A!/) - APPROVED_BIOME_EXCLUSIONS
  unless unreviewed_exclusions.empty?
    raise PolicyViolation,
          "biome.json contains unreviewed coverage exclusions: #{unreviewed_exclusions.inspect}"
  end

  rust_version = toml_string(root, "Cargo.toml", "workspace.package", "rust-version")
  toolchain = toml_string(root, "rust-toolchain.toml", "toolchain", "channel")
  unless rust_version.match?(/\A(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\z/)
    raise PolicyViolation, "Cargo workspace rust-version must be one exact stable compiler version"
  end
  unless rust_version == toolchain
    raise PolicyViolation,
          "Cargo workspace rust-version #{rust_version.inspect} must equal rust-toolchain channel #{toolchain.inspect}"
  end
end

def workflow_policy_paths(root)
  relative_directory = ".github/workflows"
  reject_symlink_components(root, relative_directory, "workflow policy directory")
  directory = File.join(root, relative_directory)
  raise PolicyViolation, "workflow policy directory is missing: #{directory}" unless File.directory?(directory)

  paths = Dir.children(directory).each_with_object([]) do |entry, result|
    suffix = File.extname(entry)
    next unless WORKFLOW_SUFFIXES.include?(suffix)

    result << File.join(relative_directory, entry)
  end.sort
  raise PolicyViolation, "trusted workflow set must not be empty" if paths.empty?

  paths
rescue SystemCallError => error
  raise PolicyViolation, "workflow policy set could not be enumerated: #{error.message}"
end

def repository_file_paths(root)
  output, _error, status = Open3.capture3(
    "git", "-C", root, "ls-files", "--cached", "--others", "--exclude-standard", "-z"
  )
  return output.split("\0").reject(&:empty?).uniq.sort if status.success?

  files = []
  Find.find(root) do |entry|
    next if entry == root

    relative = entry.delete_prefix("#{root}/")
    stat = File.lstat(entry)
    if stat.directory?
      if %w[.git node_modules target dist .pnpm-store].include?(File.basename(entry))
        Find.prune
      end
      next
    end
    files << relative
  end
  files.uniq.sort
rescue SystemCallError => error
  raise PolicyViolation, "repository policy files could not be enumerated: #{error.message}"
end

def discoverable_policy_paths(root)
  paths = repository_file_paths(root).select { |relative| discoverable_policy_path?(relative) }
  required_sets = {
    "agent guidance" => paths.select do |relative|
      %w[AGENTS.md AGENTS.override.md CLAUDE.md CLAUDE.local.md].include?(File.basename(relative))
    end,
    "Claude rules" => paths.grep(%r{\A\.claude/rules/}),
    "Claude skills" => paths.grep(%r{(?:\A|/)\.claude/skills/[^/]+/SKILL\.md\z}),
    "Codex skills" => paths.grep(%r{(?:\A|/)\.agents/skills/[^/]+/SKILL\.md\z}),
    "Codex rules" => paths.grep(%r{\A\.codex/rules/}),
    "Git hooks" => paths.grep(%r{\A\.githooks/})
  }
  required_sets.each do |label, entries|
    raise PolicyViolation, "#{label} policy set must not be empty" if entries.empty?
  end
  paths.uniq.sort
end

def compare_policy_path_sets(label, trusted, candidate)
  return if candidate == trusted

  added = candidate - trusted
  removed = trusted - candidate
  raise PolicyViolation,
        "candidate changed the trusted #{label} set; added=#{added.inspect}, removed=#{removed.inspect}; " \
        "policy changes require an explicit red/manual security review"
end

def policy_paths_for_root(root)
  (STATIC_POLICY_PATHS + workflow_policy_paths(root) + discoverable_policy_paths(root)).uniq.sort
end

def policy_paths_for_roots(trusted_root, candidate_root)
  trusted_workflows = workflow_policy_paths(trusted_root)
  candidate_workflows = workflow_policy_paths(candidate_root)
  compare_policy_path_sets("workflow file", trusted_workflows, candidate_workflows)

  trusted_discoverable = discoverable_policy_paths(trusted_root)
  candidate_discoverable = discoverable_policy_paths(candidate_root)
  compare_policy_path_sets("discoverable policy", trusted_discoverable, candidate_discoverable)

  (STATIC_POLICY_PATHS + trusted_workflows + trusted_discoverable).uniq.sort
end

def reject_local_actions_path(root, context)
  path = File.join(root, LOCAL_ACTIONS_PATH)
  return unless File.exist?(path) || File.symlink?(path)

  raise PolicyViolation,
        "#{context} contains #{LOCAL_ACTIONS_PATH}; repository-local actions are forbidden " \
        "until their transitive code and modes are explicitly trusted and pinned"
end

def validate_pinned_policy(trusted_root, candidate_root)
  raise PolicyViolation, "trusted policy root is not a directory" unless File.directory?(trusted_root)
  raise PolicyViolation, "candidate data root is not a directory" unless File.directory?(candidate_root)
  raise PolicyViolation, "candidate data root must not be a symbolic link" if File.symlink?(candidate_root)
  reject_local_actions_path(trusted_root, "trusted workflow revision")
  reject_local_actions_path(candidate_root, "candidate")

  policy_paths_for_roots(trusted_root, candidate_root).each do |relative|
    reject_symlink_components(trusted_root, relative, "trusted policy path #{relative}")
    reject_symlink_components(candidate_root, relative, "candidate policy path #{relative}")
    trusted = File.join(trusted_root, relative)
    candidate = File.join(candidate_root, relative)
    raise PolicyViolation, "trusted workflow revision lacks #{relative}" unless File.file?(trusted)
    raise PolicyViolation, "trusted policy must not be a symbolic link: #{relative}" if File.symlink?(trusted)
    raise PolicyViolation, "candidate removed trusted policy file #{relative}" unless File.file?(candidate)
    raise PolicyViolation, "candidate policy must not be a symbolic link: #{relative}" if File.symlink?(candidate)

    unless File.binread(candidate) == File.binread(trusted)
      raise PolicyViolation,
            "candidate changed trusted policy file #{relative}; policy changes require an explicit red/manual security review"
    end

    trusted_exec = File.stat(trusted).mode & 0o111
    candidate_exec = File.stat(candidate).mode & 0o111
    unless candidate_exec == trusted_exec
      raise PolicyViolation, "candidate changed executable mode for trusted policy file #{relative}"
    end
  end
end

def revision_workflow_paths(label, revision)
  output, error, status = Open3.capture3(
    "git", "ls-tree", "-r", "-z", "--name-only", "--full-tree", revision,
    "--", ".github/workflows"
  )
  unless status.success?
    raise PolicyViolation, "git could not enumerate #{label} workflows: #{error.strip}"
  end

  paths = output.split("\0").select do |path|
    File.dirname(path) == ".github/workflows" && WORKFLOW_SUFFIXES.include?(File.extname(path))
  end.sort
  raise PolicyViolation, "#{label} revision has no workflow definitions" if paths.empty?

  paths
end

def discoverable_policy_path?(path)
  basename = File.basename(path)
  return true if %w[AGENTS.md AGENTS.override.md CLAUDE.md CLAUDE.local.md].include?(basename)
  return true if path == ".githooks" || path.start_with?(".githooks/")
  # Claude discovers project policy relative to the current working directory
  # and its parents. Freeze every root or subtree .claude tree rather than
  # maintaining an incomplete list of settings, hooks, agents, or commands.
  return true if path.match?(%r{(?:\A|/)\.claude(?:\z|/)})
  # Apply the same fail-closed rule to Codex project configuration. This keeps
  # a newly supported nested config surface from becoming an unreviewed bypass.
  return true if path.match?(%r{(?:\A|/)\.codex(?:\z|/)})
  return true if path.match?(%r{(?:\A|/)\.agents\z})
  return true if path.match?(%r{(?:\A|/)\.agents/skills(?:\z|/[^/]+(?:/.*)?\z)})
  return true if %w[.mcp.json .worktreeinclude].include?(path)

  false
end

def revision_discoverable_policy_entries(label, revision)
  output, error, status = Open3.capture3(
    "git", "ls-tree", "-r", "-z", "--full-tree", revision
  )
  unless status.success?
    raise PolicyViolation, "git could not enumerate #{label} discoverable policy: #{error.strip}"
  end

  entries = output.split("\0").each_with_object({}) do |record, result|
    metadata, path = record.split("\t", 2)
    next unless path && discoverable_policy_path?(path)

    mode, type, object = metadata.split(" ", 3)
    unless %w[100644 100755].include?(mode) && type == "blob" && object&.match?(/\A[0-9a-f]+\z/)
      raise PolicyViolation,
            "#{label} discoverable policy #{path} must be a regular Git blob, got #{metadata.inspect}"
    end
    result[path] = record
  end

  categories = {
    "agent guidance" => entries.keys.select do |path|
      %w[AGENTS.md AGENTS.override.md CLAUDE.md CLAUDE.local.md].include?(File.basename(path))
    end,
    "Claude rules" => entries.keys.grep(%r{\A\.claude/rules/.+\.md\z}),
    "Claude skills" => entries.keys.grep(%r{(?:\A|/)\.claude/skills/[^/]+/SKILL\.md\z}),
    "Codex skills" => entries.keys.grep(%r{(?:\A|/)\.agents/skills/[^/]+/SKILL\.md\z}),
    "Codex rules" => entries.keys.grep(%r{\A\.codex/rules/[^/]+\.rules\z}),
    "Git hooks" => entries.keys.grep(%r{\A\.githooks/})
  }
  categories.each do |category, paths|
    raise PolicyViolation, "#{label} #{category} policy set must not be empty" if paths.empty?
  end
  entries
end

def reject_revision_local_actions(label, revision)
  output, error, status = Open3.capture3(
    "git", "ls-tree", "-z", "--full-tree", revision, "--", LOCAL_ACTIONS_PATH
  )
  unless status.success?
    raise PolicyViolation, "git could not inspect #{label} local-action boundary: #{error.strip}"
  end
  return if output.empty?

  raise PolicyViolation,
        "#{label} revision contains #{LOCAL_ACTIONS_PATH}; repository-local actions are forbidden " \
        "until their transitive code and modes are explicitly trusted and pinned"
end

def validate_revision_policy(trusted_revision, candidate_revision)
  revisions = {
    "trusted workflow" => trusted_revision,
    "candidate" => candidate_revision
  }
  revisions.each do |label, revision|
    unless revision.match?(/\A[0-9a-f]{40}\z/)
      raise PolicyViolation, "#{label} revision must be an exact 40-character commit SHA"
    end
    reject_revision_local_actions(label, revision)
  end

  trusted_workflows = revision_workflow_paths("trusted workflow", trusted_revision)
  candidate_workflows = revision_workflow_paths("candidate", candidate_revision)
  compare_policy_path_sets("workflow file", trusted_workflows, candidate_workflows)

  trusted_discoverable = revision_discoverable_policy_entries("trusted workflow", trusted_revision)
  candidate_discoverable = revision_discoverable_policy_entries("candidate", candidate_revision)
  compare_policy_path_sets(
    "discoverable policy",
    trusted_discoverable.keys.sort,
    candidate_discoverable.keys.sort
  )

  (STATIC_POLICY_PATHS + trusted_workflows + trusted_discoverable.keys).uniq.sort.each do |relative|
    entries = revisions.to_h do |label, revision|
      output, error, status = Open3.capture3(
        "git", "ls-tree", "-z", "--full-tree", revision, "--", relative
      )
      unless status.success?
        raise PolicyViolation, "git could not inspect #{label} policy #{relative}: #{error.strip}"
      end
      raise PolicyViolation, "#{label} revision lacks policy blob #{relative}" if output.empty?

      [label, output]
    end
    next if entries.fetch("trusted workflow") == entries.fetch("candidate")

    raise PolicyViolation,
          "candidate changed trusted policy blob or mode #{relative}; policy changes require an explicit red/manual security review"
  end
end

def validate_candidate(
  trusted_root,
  candidate_root,
  trusted_revision: nil,
  candidate_revision: nil
)
  if trusted_revision || candidate_revision
    unless trusted_revision && candidate_revision
      raise PolicyViolation, "trusted and candidate revisions must be supplied together"
    end
    validate_revision_policy(trusted_revision, candidate_revision)
  end
  validate_pinned_policy(trusted_root, candidate_root)
  validate_repository_configuration(candidate_root)
  reject_symlink_components(
    candidate_root,
    ".github/workflows/branch-flow.yml",
    "candidate workflow path"
  )
  validate_file(File.join(candidate_root, ".github/workflows/branch-flow.yml"))
  reject_symlink_components(
    candidate_root,
    ".github/workflows/labeler.yml",
    "candidate labeler workflow path"
  )
  validate_labeler_file(File.join(candidate_root, ".github/workflows/labeler.yml"))
end

def check(path, quiet: false)
  validate_file(path)
  puts "branch workflow policy OK: #{path}" unless quiet
  true
rescue PolicyViolation, Psych::Exception, SystemCallError => error
  warn "branch workflow policy FAIL: #{error.message}" unless quiet
  false
end

def check_labeler(path, quiet: false)
  validate_labeler_file(path)
  puts "labeler workflow policy OK: #{path}" unless quiet
  true
rescue PolicyViolation, Psych::Exception, SystemCallError => error
  warn "labeler workflow policy FAIL: #{error.message}" unless quiet
  false
end

def check_candidate(
  trusted_root,
  candidate_root,
  trusted_revision: nil,
  candidate_revision: nil,
  quiet: false
)
  validate_candidate(
    trusted_root,
    candidate_root,
    trusted_revision: trusted_revision,
    candidate_revision: candidate_revision
  )
  puts "branch workflow and policy blobs OK: #{candidate_root}" unless quiet
  true
rescue PolicyViolation, Psych::Exception, SystemCallError => error
  warn "branch workflow policy FAIL: #{error.message}" unless quiet
  false
end

def self_test(default_path)
  unless check(default_path, quiet: true)
    warn "repository branch-flow workflow must pass before its adversarial tests run"
    return false
  end

  canonical = File.read(default_path)
  labeler_path = File.expand_path("../.github/workflows/labeler.yml", __dir__)
  unless check_labeler(labeler_path, quiet: true)
    warn "repository labeler workflow must pass before its adversarial tests run"
    return false
  end
  labeler_canonical = File.read(labeler_path)
  passed = 0
  failed = 0

  cases = {
    "pull_request cannot replace pull_request_target" => ["pull_request_target:", "pull_request:"],
    "an extra trigger is rejected" => ["  pull_request_target:\n", "  push:\n  pull_request_target:\n"],
    "write permission is rejected" => ["  contents: read", "  contents: write"],
    "an extra permission is rejected" => ["  contents: read\n", "  contents: read\n  pull-requests: read\n"],
    "a mutable checkout reference is rejected" => [CHECKOUT, "actions/checkout@v7"],
    "policy must come from the workflow definition SHA" => [WORKFLOW_SHA, BASE_SHA],
    "checkout credentials cannot persist" => ["persist-credentials: false", "persist-credentials: true"],
    "the trusted checkout must retain repository history" => ["          fetch-depth: 0\n", "          fetch-depth: 1\n"],
    "a candidate checkout path is rejected" => ["          fetch-depth: 0\n", "          path: candidate\n          fetch-depth: 0\n"],
    "cache inputs are rejected" => ["          fetch-depth: 0\n", "          fetch-depth: 0\n          cache: true\n"],
    "repository secret contexts are rejected" => ["          GH_TOKEN: ${{ github.token }}\n", "          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n"],
    "candidate policy cannot be executed" => ["$GITHUB_WORKSPACE/scripts/check-protected-paths.sh", "$candidate_root/scripts/check-protected-paths.sh"],
    "the PR head ref is fetched as data" => ["+refs/pull/$PR_NUMBER/head:refs/remotes/pull/$PR_NUMBER/head", "+refs/heads/main:refs/remotes/pull/$PR_NUMBER/head"],
    "shallow candidate history is rejected" => ["fetch --no-tags --no-recurse-submodules origin", "fetch --no-tags --no-recurse-submodules --depth=1 origin"],
    "the fetched head must equal the event head" => ["[ \"$fetched\" = \"$HEAD_SHA\" ]", "[ -n \"$fetched\" ]"],
    "candidate worktree hooks remain disabled" => ["core.hooksPath=/dev/null", "core.hooksPath=.githooks"],
    "the candidate path cannot flow through GITHUB_ENV" => ["          candidate_root=\"$RUNNER_TEMP/candidate\"\n", "          candidate_root=\"$RUNNER_TEMP/candidate\"\n          printf 'CANDIDATE_ROOT=%s\\n' \"$candidate_root\" >> \"$GITHUB_ENV\"\n"],
    "a fail-open command cannot precede policy" => ["          ruby \"$policy\"", "          exit 0\n          ruby \"$policy\""],
    "the protected-paths job cannot be renamed away" => ["  protected-paths:\n", "  optional-paths:\n"],
    "job-level permission overrides are rejected" => ["    runs-on: ubuntu-latest\n", "    permissions: {}\n    runs-on: ubuntu-latest\n"],
    "the exact event set is retained" => ["types: [opened, edited, reopened, synchronize]", "types: [opened, synchronize]"],
    "repository-local actions cannot replace checkout" => [CHECKOUT, "./candidate/.github/actions/checkout"],
    "the workflow self-policy step cannot disappear" => ["The next workflow retains this trusted-workflow boundary", "The next workflow skips its trusted-workflow boundary"],
    "promotion titles cannot bypass attribution" => ["          printf '%s\\n%s' \"$TITLE\" \"$PR_BODY\" > \"$message_file\"\n", "          printf '%s' \"$PR_BODY\" > \"$message_file\"\n"],
    "duplicate YAML keys are rejected" => ["permissions:\n", "permissions:\n  contents: read\npermissions:\n"],
    "YAML aliases are rejected" => ["name: branch-flow", "name: &workflow_name branch-flow\nx-copy: *workflow_name"]
  }

  puts "check-branch-workflow-policy.rb — trusted pull-request boundary"
  Dir.mktmpdir("branch-workflow-policy") do |directory|
    fixture = File.join(directory, "branch-flow.yml")
    cases.each do |label, (needle, replacement)|
      mutated = canonical.sub(needle, replacement)
      if mutated == canonical
        puts "  FAIL  fixture mutation did not apply: #{label}"
        failed += 1
        next
      end
      File.write(fixture, mutated)
      if check(fixture, quiet: true)
        puts "  FAIL  #{label}"
        failed += 1
      else
        puts "  ok    #{label}"
        passed += 1
      end
    end

    File.write(fixture, "name: [\n")
    if check(fixture, quiet: true)
      puts "  FAIL  invalid YAML fails closed"
      failed += 1
    else
      puts "  ok    invalid YAML fails closed"
      passed += 1
    end

    labeler_cases = {
      "labeler code must come from the workflow definition SHA" => [WORKFLOW_SHA, BASE_SHA],
      "labeler checkout credentials cannot persist" => ["persist-credentials: false", "persist-credentials: true"],
      "labeler cannot move its write token onto pull_request" => ["  pull_request_target:\n", "  pull_request:\n"],
      "labeler content access cannot become writable" => ["  contents: read", "  contents: write"],
      "labeler cannot add a candidate checkout path" => ["          persist-credentials: false\n", "          persist-credentials: false\n          path: candidate\n"],
      "labeler action cannot use a mutable reference" => [LABELER_ACTION, "actions/labeler@v7"],
      "labeler cannot replace the trusted checkout" => [CHECKOUT, LABELER_ACTION]
    }
    labeler_fixture = File.join(directory, "labeler.yml")
    labeler_cases.each do |label, (needle, replacement)|
      mutated = labeler_canonical.sub(needle, replacement)
      if mutated == labeler_canonical
        puts "  FAIL  fixture mutation did not apply: #{label}"
        failed += 1
        next
      end
      File.write(labeler_fixture, mutated)
      if check_labeler(labeler_fixture, quiet: true)
        puts "  FAIL  #{label}"
        failed += 1
      else
        puts "  ok    #{label}"
        passed += 1
      end
    end

    trusted_root = File.expand_path("..", __dir__)
    candidate_root = File.join(directory, "candidate")
    policy_paths = policy_paths_for_root(trusted_root)
    policy_paths.each do |relative|
      source = File.join(trusted_root, relative)
      destination = File.join(candidate_root, relative)
      FileUtils.mkdir_p(File.dirname(destination))
      FileUtils.cp(source, destination, preserve: true)
    end
    (STRUCTURAL_POLICY_PATHS - policy_paths).each do |relative|
      source = File.join(trusted_root, relative)
      destination = File.join(candidate_root, relative)
      FileUtils.mkdir_p(File.dirname(destination))
      FileUtils.cp(source, destination, preserve: true)
    end

    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  ok    unchanged candidate policy blobs match the trusted workflow revision"
      passed += 1
    else
      puts "  FAIL  unchanged candidate policy blobs match the trusted workflow revision"
      failed += 1
    end

    self_node_version = File.read(File.join(trusted_root, ".nvmrc")).strip
    self_rust_version = toml_string(
      trusted_root, "Cargo.toml", "workspace.package", "rust-version"
    )
    self_package_manager = json_policy_file(trusted_root, "package.json")["packageManager"]
    self_node_major = self_node_version.split(".").first
    self_types_node = yaml_policy_file(
      trusted_root, "pnpm-workspace.yaml"
    ).dig("overrides", "@types/node").to_s
    structural_cases = {
      "package manager cannot become mutable" => [
        "package.json", "\"packageManager\": \"#{self_package_manager}\"",
        '"packageManager": "pnpm@latest"'
      ],
      "Node engine cannot drift from .nvmrc" => [
        "package.json", "\"node\": \"#{self_node_version}\"", '"node": "0.0.0"'
      ],
      "the root Biome lint script cannot become fail-open" => [
        "package.json", '"lint": "biome ci --error-on-warnings ."',
        '"lint": "biome check . || true"'
      ],
      "recursive web builds cannot skip missing scripts" => [
        "package.json", '"build:web": "just build-web"',
        '"build:web": "pnpm -r build"'
      ],
      "pnpm Node resolution cannot drift from .nvmrc" => [
        "pnpm-workspace.yaml", "nodeVersion: #{self_node_version}", "nodeVersion: 0.0.0"
      ],
      "dependency engine checks cannot become advisory" => [
        "pnpm-workspace.yaml", "engineStrict: true", "engineStrict: false"
      ],
      "workspace coverage cannot omit the terminal app" => [
        "pnpm-workspace.yaml", "  - apps/terminal\n", ""
      ],
      "dependency build scripts cannot gain unreviewed execution" => [
        "pnpm-workspace.yaml", "  esbuild: true", "  esbuild: false"
      ],
      "Node declarations cannot compile against a different major" => [
        "pnpm-workspace.yaml", "\"@types/node\": #{self_node_major}.",
        "\"@types/node\": #{self_node_major.to_i + 1}."
      ],
      "app Node declarations cannot drift from the workspace override" => [
        "apps/terminal/package.json", "\"@types/node\": \"#{self_types_node}\"",
        '"@types/node": "0.0.0"'
      ],
      "Biome linting cannot be disabled" => [
        "biome.json", "\"linter\": {\n    \"enabled\": true",
        "\"linter\": {\n    \"enabled\": false"
      ],
      "Biome cannot exclude an application tree" => [
        "biome.json", '"apps/**",', '"apps/**",\n      "!apps/**",'
      ],
      "Biome cannot hide future code in public directories" => [
        "biome.json", '"!**/public/**/*.svg"', '"!**/public"'
      ],
      "Cargo's compiler contract cannot drift below the tested toolchain" => [
        "Cargo.toml", "rust-version = \"#{self_rust_version}\"", 'rust-version = "0.0.0"'
      ]
    }
    structural_cases.each do |label, (relative, needle, replacement)|
      candidate_policy = File.join(candidate_root, relative)
      original = File.read(candidate_policy)
      mutated = original.sub(needle, replacement)
      if mutated == original
        puts "  FAIL  structural fixture mutation did not apply: #{label}"
        failed += 1
        next
      end
      File.write(candidate_policy, mutated)
      if check_candidate(trusted_root, candidate_root, quiet: true)
        puts "  FAIL  #{label}"
        failed += 1
      else
        puts "  ok    #{label}"
        passed += 1
      end
      File.write(candidate_policy, original)
    end

    discoverable_additions = {
      "candidate-only nested AGENTS.md" => ["docs/adversarial/AGENTS.md", "# candidate policy\n"],
      "candidate-only root AGENTS.override.md" => ["AGENTS.override.md", "# candidate override\n"],
      "candidate-only nested AGENTS.override.md" => ["docs/adversarial/AGENTS.override.md", "# candidate override\n"],
      "candidate-only root CLAUDE.local.md" => ["CLAUDE.local.md", "# candidate local policy\n"],
      "candidate-only nested CLAUDE.local.md" => ["docs/adversarial/CLAUDE.local.md", "# candidate local policy\n"],
      "candidate-only nested Claude rule" => [".claude/rules/adversarial/unsafe.md", "# candidate rule\n"],
      "candidate-only subtree Claude settings" => ["packages/adversarial/.claude/settings.json", "{}\n"],
      "candidate-only subtree Claude local settings" => ["packages/adversarial/.claude/settings.local.json", "{}\n"],
      "candidate-only subtree Claude agent" => ["packages/adversarial/.claude/agents/unsafe.md", "# candidate agent\n"],
      "candidate-only subtree Claude rule" => ["packages/adversarial/.claude/rules/unsafe.md", "# candidate rule\n"],
      "candidate-only subtree Claude hook" => ["packages/adversarial/.claude/hooks/unsafe.sh", "#!/usr/bin/env bash\nexit 0\n"],
      "candidate-only Claude skill" => [".claude/skills/adversarial/SKILL.md", "---\nname: adversarial\n---\n"],
      "candidate-only nested Claude skill" => ["packages/adversarial/.claude/skills/unsafe/SKILL.md", "---\nname: unsafe\n---\n"],
      "candidate-only Codex skill" => [".agents/skills/adversarial/SKILL.md", "---\nname: adversarial\n---\n"],
      "candidate-only nested Codex skill" => ["packages/adversarial/.agents/skills/unsafe/SKILL.md", "---\nname: unsafe\n---\n"],
      "candidate-only Claude command" => [".claude/commands/unsafe.md", "# candidate command\n"],
      "candidate-only namespaced Claude command" => [".claude/commands/team/unsafe.md", "# candidate command\n"],
      "candidate-only Claude agent" => [".claude/agents/unsafe.md", "# candidate agent\n"],
      "candidate-only Claude output style" => [".claude/output-styles/unsafe.md", "# candidate style\n"],
      "candidate-only Claude agent memory" => [".claude/agent-memory/unsafe/state.md", "candidate memory\n"],
      "candidate-only Codex agent" => [".codex/agents/explorer.toml", "name = \"unsafe\"\n"],
      "candidate-only Codex rule" => [".codex/rules/allow.rules", "prefix_rule(pattern=[\"git\"], decision=\"allow\")\n"],
      "candidate-only subtree Codex config" => ["packages/adversarial/.codex/config.toml", "sandbox_mode = \"danger-full-access\"\n"],
      "candidate-only Claude settings override" => [".claude/settings.local.json", "{}\n"],
      "candidate-only MCP configuration" => [".mcp.json", "{}\n"],
      "candidate-only worktree include policy" => [".worktreeinclude", ".env\n"],
      "candidate-only executable Git hook" => [".githooks/post-checkout", "#!/usr/bin/env bash\nexit 0\n"]
    }
    discoverable_additions.each do |label, (relative, content)|
      added = File.join(candidate_root, relative)
      FileUtils.mkdir_p(File.dirname(added))
      File.write(added, content)
      if relative.start_with?(".githooks/") || relative.match?(%r{(?:\A|/)\.claude/hooks/})
        File.chmod(0o755, added)
      end
      if check_candidate(trusted_root, candidate_root, quiet: true)
        puts "  FAIL  #{label} is rejected"
        failed += 1
      else
        puts "  ok    #{label} is rejected"
        passed += 1
      end
      FileUtils.rm(added)
      parent = File.dirname(added)
      while parent != candidate_root && File.directory?(parent) && Dir.empty?(parent)
        Dir.rmdir(parent)
        parent = File.dirname(parent)
      end
    end

    codex_support = File.join(
      candidate_root, ".agents/skills/add-migration/adversarial-support.md"
    )
    File.write(codex_support, "candidate support content\n")
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  candidate-only Codex skill support content is rejected"
      failed += 1
    else
      puts "  ok    candidate-only Codex skill support content is rejected"
      passed += 1
    end
    FileUtils.rm(codex_support)

    codex_support_symlink = File.join(
      candidate_root, ".agents/skills/add-migration/adversarial-link"
    )
    File.symlink("../../../scripts/validate-branch-flow.sh", codex_support_symlink)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  symbolic-link Codex skill support content is rejected"
      failed += 1
    else
      puts "  ok    symbolic-link Codex skill support content is rejected"
      passed += 1
    end
    FileUtils.rm(codex_support_symlink)

    claude_support = File.join(
      candidate_root, ".claude/skills/add-migration/adversarial-support.md"
    )
    File.write(claude_support, "candidate support content\n")
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  candidate-only Claude skill support content is rejected"
      failed += 1
    else
      puts "  ok    candidate-only Claude skill support content is rejected"
      passed += 1
    end
    FileUtils.rm(claude_support)

    claude_support_symlink = File.join(
      candidate_root, ".claude/skills/add-migration/adversarial-link"
    )
    File.symlink("../../../scripts/validate-branch-flow.sh", claude_support_symlink)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  symbolic-link Claude skill support content is rejected"
      failed += 1
    else
      puts "  ok    symbolic-link Claude skill support content is rejected"
      passed += 1
    end
    FileUtils.rm(claude_support_symlink)

    codex_rule_symlink = File.join(candidate_root, ".codex/rules/allow.rules")
    File.symlink("../../scripts/validate-branch-flow.sh", codex_rule_symlink)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  symbolic-link Codex rule is rejected"
      failed += 1
    else
      puts "  ok    symbolic-link Codex rule is rejected"
      passed += 1
    end
    FileUtils.rm(codex_rule_symlink)

    candidate_hook_symlink = File.join(candidate_root, ".githooks/post-merge")
    File.symlink("../scripts/validate-branch-flow.sh", candidate_hook_symlink)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  candidate-only symbolic-link Git hook is rejected"
      failed += 1
    else
      puts "  ok    candidate-only symbolic-link Git hook is rejected"
      passed += 1
    end
    FileUtils.rm(candidate_hook_symlink)

    component_symlinks = {
      "symbolic-link Claude command root" => [".claude/commands", "../docs"],
      "symbolic-link nested Claude policy root" => [
        "packages/adversarial/.claude", "../../.claude"
      ],
      "symbolic-link nested Codex policy root" => [
        "packages/adversarial/.agents", "../../.agents"
      ],
      "symbolic-link Codex agent root" => [".codex/agents", "../docs"]
    }
    component_symlinks.each do |label, (relative, target)|
      added = File.join(candidate_root, relative)
      FileUtils.mkdir_p(File.dirname(added))
      File.symlink(target, added)
      if check_candidate(trusted_root, candidate_root, quiet: true)
        puts "  FAIL  candidate-only #{label} is rejected"
        failed += 1
      else
        puts "  ok    candidate-only #{label} is rejected"
        passed += 1
      end
      FileUtils.rm(added)
      parent = File.dirname(added)
      while parent != candidate_root && File.directory?(parent) && Dir.empty?(parent)
        Dir.rmdir(parent)
        parent = File.dirname(parent)
      end
    end

    local_actions = File.join(candidate_root, LOCAL_ACTIONS_PATH)
    FileUtils.mkdir_p(local_actions)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  adding an empty repository-local action directory is rejected"
      failed += 1
    else
      puts "  ok    adding an empty repository-local action directory is rejected"
      passed += 1
    end
    FileUtils.rm_rf(local_actions)

    FileUtils.mkdir_p(local_actions)
    local_action = File.join(local_actions, "action.yml")
    File.write(local_action, "name: candidate action\nruns:\n  using: composite\n  steps: []\n")
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  adding repository-local action content is rejected"
      failed += 1
    else
      puts "  ok    adding repository-local action content is rejected"
      passed += 1
    end
    FileUtils.rm_rf(local_actions)

    File.symlink("../scripts", local_actions)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  adding a repository-local action symlink is rejected"
      failed += 1
    else
      puts "  ok    adding a repository-local action symlink is rejected"
      passed += 1
    end
    FileUtils.rm(local_actions)

    FileUtils.mkdir_p(local_actions)
    File.write(local_action, "#!/usr/bin/env bash\nexit 0\n")
    File.chmod(0o755, local_action)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  executable repository-local action content is rejected"
      failed += 1
    else
      puts "  ok    executable repository-local action content is rejected"
      passed += 1
    end
    FileUtils.rm_rf(local_actions)

    policy_paths.each do |relative|
      candidate_policy = File.join(candidate_root, relative)
      original = File.binread(candidate_policy)
      File.binwrite(candidate_policy, original + "\n# adversarial policy-only change\n")
      if check_candidate(trusted_root, candidate_root, quiet: true)
        puts "  FAIL  policy-only change is rejected: #{relative}"
        failed += 1
      else
        puts "  ok    policy-only change is rejected: #{relative}"
        passed += 1
      end
      File.binwrite(candidate_policy, original)
      File.chmod(File.stat(File.join(trusted_root, relative)).mode & 0o777, candidate_policy)
    end

    added_workflow = File.join(candidate_root, ".github/workflows/candidate-only.yaml")
    File.write(added_workflow, "name: candidate only\non: push\n")
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  adding a candidate-only workflow is rejected"
      failed += 1
    else
      puts "  ok    adding a candidate-only workflow is rejected"
      passed += 1
    end
    FileUtils.rm(added_workflow)

    mode_relative = ".githooks/pre-commit"
    mode_path = File.join(candidate_root, mode_relative)
    original_mode = File.stat(mode_path).mode & 0o777
    File.chmod(original_mode ^ 0o100, mode_path)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  changing a trusted helper's executable mode is rejected"
      failed += 1
    else
      puts "  ok    changing a trusted helper's executable mode is rejected"
      passed += 1
    end
    File.chmod(original_mode, mode_path)

    removed = File.join(candidate_root, workflow_policy_paths(trusted_root).last)
    FileUtils.rm(removed)
    if check_candidate(trusted_root, candidate_root, quiet: true)
      puts "  FAIL  removing a trusted policy blob is rejected"
      failed += 1
    else
      puts "  ok    removing a trusted policy blob is rejected"
      passed += 1
    end

    revision_repo = File.join(directory, "revision-policy")
    FileUtils.mkdir_p(revision_repo)
    run_git = lambda do |*arguments|
      output, error, status = Open3.capture3("git", "-C", revision_repo, *arguments)
      unless status.success?
        raise PolicyViolation, "self-test git #{arguments.join(' ')} failed: #{error.strip}#{output.strip}"
      end
      output.strip
    end
    run_git.call("init", "-q")
    run_git.call("config", "user.name", "policy-test")
    run_git.call("config", "user.email", "policy-test@example.invalid")
    policy_paths.each do |relative|
      destination = File.join(revision_repo, relative)
      FileUtils.mkdir_p(File.dirname(destination))
      FileUtils.cp(File.join(trusted_root, relative), destination, preserve: true)
    end
    run_git.call("add", "--", *policy_paths)
    run_git.call("commit", "-q", "--no-verify", "-m", "trusted policy")
    trusted_revision = run_git.call("rev-parse", "HEAD")
    File.write(File.join(revision_repo, "ordinary.txt"), "ordinary candidate data\n")
    run_git.call("add", "ordinary.txt")
    run_git.call("commit", "-q", "--no-verify", "-m", "ordinary change")
    unchanged_revision = run_git.call("rev-parse", "HEAD")
    begin
      Dir.chdir(revision_repo) do
        validate_revision_policy(trusted_revision, unchanged_revision)
      end
      puts "  ok    Git tree comparison allows an unrelated candidate change"
      passed += 1
    rescue PolicyViolation
      puts "  FAIL  Git tree comparison allows an unrelated candidate change"
      failed += 1
    end

    changed_relative = STATIC_POLICY_PATHS.first
    changed_policy = File.join(revision_repo, changed_relative)
    File.open(changed_policy, "ab") { |file| file.write("\n# changed policy blob\n") }
    run_git.call("add", "--", changed_relative)
    run_git.call("commit", "-q", "--no-verify", "-m", "poison policy")
    changed_revision = run_git.call("rev-parse", "HEAD")
    begin
      Dir.chdir(revision_repo) do
        validate_revision_policy(trusted_revision, changed_revision)
      end
      puts "  FAIL  Git tree comparison rejects a policy-only commit"
      failed += 1
    rescue PolicyViolation
      puts "  ok    Git tree comparison rejects a policy-only commit"
      passed += 1
    end

    FileUtils.cp(File.join(trusted_root, changed_relative), changed_policy, preserve: true)
    run_git.call("add", "--", changed_relative)
    run_git.call("commit", "-q", "--no-verify", "-m", "restore policy")

    revision_local_action = File.join(revision_repo, LOCAL_ACTIONS_PATH, "action.yml")
    FileUtils.mkdir_p(File.dirname(revision_local_action))
    File.write(revision_local_action, "name: candidate action\nruns:\n  using: composite\n  steps: []\n")
    File.chmod(0o755, revision_local_action)
    run_git.call("add", "--", LOCAL_ACTIONS_PATH)
    run_git.call("commit", "-q", "--no-verify", "-m", "add local action")
    local_action_revision = run_git.call("rev-parse", "HEAD")
    begin
      Dir.chdir(revision_repo) do
        validate_revision_policy(trusted_revision, local_action_revision)
      end
      puts "  FAIL  Git tree comparison rejects repository-local action code and mode"
      failed += 1
    rescue PolicyViolation
      puts "  ok    Git tree comparison rejects repository-local action code and mode"
      passed += 1
    end

    FileUtils.rm_rf(File.join(revision_repo, LOCAL_ACTIONS_PATH))
    run_git.call("add", "--", LOCAL_ACTIONS_PATH)
    run_git.call("commit", "-q", "--no-verify", "-m", "remove local action")

    revision_discoverable_change = ".claude/rules/security.md"
    revision_discoverable_path = File.join(revision_repo, revision_discoverable_change)
    File.open(revision_discoverable_path, "ab") do |file|
      file.write("\n# changed discoverable policy blob\n")
    end
    run_git.call("add", "--", revision_discoverable_change)
    run_git.call("commit", "-q", "--no-verify", "-m", "poison discoverable policy")
    discoverable_change_revision = run_git.call("rev-parse", "HEAD")
    begin
      Dir.chdir(revision_repo) do
        validate_revision_policy(trusted_revision, discoverable_change_revision)
      end
      puts "  FAIL  Git tree comparison rejects a changed discoverable policy blob"
      failed += 1
    rescue PolicyViolation
      puts "  ok    Git tree comparison rejects a changed discoverable policy blob"
      passed += 1
    end

    FileUtils.cp(
      File.join(trusted_root, revision_discoverable_change),
      revision_discoverable_path,
      preserve: true
    )
    run_git.call("add", "--", revision_discoverable_change)
    run_git.call("commit", "-q", "--no-verify", "-m", "restore discoverable policy")

    revision_discoverable_additions = {
      "nested AGENTS override" => [
        "docs/adversarial/AGENTS.override.md", "# candidate override\n", false
      ],
      "namespaced Claude command" => [
        ".claude/commands/team/unsafe.md", "# candidate command\n", false
      ],
      "subtree Claude settings" => [
        "packages/adversarial/.claude/settings.json", "{}\n", false
      ],
      "subtree Claude agent" => [
        "packages/adversarial/.claude/agents/unsafe.md", "# candidate agent\n", false
      ],
      "subtree Claude rule" => [
        "packages/adversarial/.claude/rules/unsafe.md", "# candidate rule\n", false
      ],
      "subtree Claude hook" => [
        "packages/adversarial/.claude/hooks/unsafe.sh",
        "#!/usr/bin/env bash\nexit 0\n",
        false
      ],
      "nested Claude skill" => [
        "packages/adversarial/.claude/skills/unsafe/SKILL.md",
        "---\nname: unsafe\n---\n",
        false
      ],
      "nested Codex skill" => [
        "packages/adversarial/.agents/skills/unsafe/SKILL.md",
        "---\nname: unsafe\n---\n",
        false
      ],
      "Codex allow rule" => [
        ".codex/rules/allow.rules",
        "prefix_rule(pattern=[\"git\"], decision=\"allow\")\n",
        false
      ],
      "subtree Codex config" => [
        "packages/adversarial/.codex/config.toml",
        "sandbox_mode = \"danger-full-access\"\n",
        false
      ],
      "tracked Claude settings override" => [
        ".claude/settings.local.json", "{}\n", false
      ],
      "symbolic-link Git hook" => [
        ".githooks/post-merge", "../scripts/validate-branch-flow.sh", true
      ],
      "symbolic-link Claude command root" => [
        ".claude/commands", "../docs", true
      ],
      "symbolic-link nested Claude policy root" => [
        "packages/adversarial/.claude", "../../.claude", true
      ],
      "symbolic-link Codex agent root" => [
        ".codex/agents", "../docs", true
      ]
    }
    revision_discoverable_additions.each do |label, (relative, payload, symlink)|
      added = File.join(revision_repo, relative)
      FileUtils.mkdir_p(File.dirname(added))
      if symlink
        File.symlink(payload, added)
      else
        File.write(added, payload)
        if relative.match?(%r{(?:\A|/)\.claude/hooks/})
          File.chmod(0o755, added)
        end
      end
      # Some higher-priority local-policy filenames are intentionally ignored
      # for normal development. A candidate can still force-track them, so the
      # Git-tree regression must exercise that exact attack path.
      run_git.call("add", "-f", "--", relative)
      run_git.call("commit", "-q", "--no-verify", "-m", "add #{label}")
      candidate_revision = run_git.call("rev-parse", "HEAD")
      begin
        Dir.chdir(revision_repo) do
          validate_revision_policy(trusted_revision, candidate_revision)
        end
        puts "  FAIL  Git tree comparison rejects candidate-only #{label}"
        failed += 1
      rescue PolicyViolation
        puts "  ok    Git tree comparison rejects candidate-only #{label}"
        passed += 1
      end

      FileUtils.rm(added)
      run_git.call("add", "--", relative)
      run_git.call("commit", "-q", "--no-verify", "-m", "remove #{label}")
      parent = File.dirname(added)
      while parent != revision_repo && File.directory?(parent) && Dir.empty?(parent)
        Dir.rmdir(parent)
        parent = File.dirname(parent)
      end
    end

    added_revision_workflow = File.join(revision_repo, ".github/workflows/candidate-only.yaml")
    File.write(added_revision_workflow, "name: candidate only\non: push\n")
    run_git.call("add", "--", ".github/workflows/candidate-only.yaml")
    run_git.call("commit", "-q", "--no-verify", "-m", "add workflow")
    added_workflow_revision = run_git.call("rev-parse", "HEAD")
    begin
      Dir.chdir(revision_repo) do
        validate_revision_policy(trusted_revision, added_workflow_revision)
      end
      puts "  FAIL  Git tree comparison rejects a candidate-only workflow"
      failed += 1
    rescue PolicyViolation
      puts "  ok    Git tree comparison rejects a candidate-only workflow"
      passed += 1
    end

    FileUtils.rm(added_revision_workflow)
    run_git.call("add", "--", ".github/workflows/candidate-only.yaml")
    run_git.call("commit", "-q", "--no-verify", "-m", "remove candidate-only workflow")

    revision_mode_path = File.join(revision_repo, mode_relative)
    revision_mode = File.stat(revision_mode_path).mode & 0o777
    File.chmod(revision_mode ^ 0o100, revision_mode_path)
    run_git.call("add", "--", mode_relative)
    run_git.call("commit", "-q", "--no-verify", "-m", "change policy mode")
    changed_mode_revision = run_git.call("rev-parse", "HEAD")
    begin
      Dir.chdir(revision_repo) do
        validate_revision_policy(trusted_revision, changed_mode_revision)
      end
      puts "  FAIL  Git tree comparison rejects a policy mode change"
      failed += 1
    rescue PolicyViolation
      puts "  ok    Git tree comparison rejects a policy mode change"
      passed += 1
    end

    File.chmod(revision_mode, revision_mode_path)
    run_git.call("add", "--", mode_relative)
    run_git.call("commit", "-q", "--no-verify", "-m", "restore policy mode")

    removed_workflow_relative = workflow_policy_paths(trusted_root).last
    FileUtils.rm(File.join(revision_repo, removed_workflow_relative))
    run_git.call("add", "--", removed_workflow_relative)
    run_git.call("commit", "-q", "--no-verify", "-m", "remove trusted workflow")
    removed_workflow_revision = run_git.call("rev-parse", "HEAD")
    begin
      Dir.chdir(revision_repo) do
        validate_revision_policy(trusted_revision, removed_workflow_revision)
      end
      puts "  FAIL  Git tree comparison rejects a removed trusted workflow"
      failed += 1
    rescue PolicyViolation
      puts "  ok    Git tree comparison rejects a removed trusted workflow"
      passed += 1
    end
  end

  puts "\n#{passed} passed, #{failed} failed"
  failed.zero?
end

default_path = File.expand_path("../.github/workflows/branch-flow.yml", __dir__)
trusted_root = File.expand_path("..", __dir__)
if ARGV == ["--self-test"]
  exit(self_test(default_path) ? 0 : 1)
end
if ARGV.length == 2 && ARGV.first == "--candidate-root"
  exit(check_candidate(trusted_root, ARGV.last) ? 0 : 1)
end
if ARGV.length == 6 && ARGV[0] == "--candidate-root" &&
   ARGV[2] == "--trusted-revision" && ARGV[4] == "--candidate-revision"
  exit(
    check_candidate(
      trusted_root,
      ARGV[1],
      trusted_revision: ARGV[3],
      candidate_revision: ARGV[5]
    ) ? 0 : 1
  )
end
if ARGV.length > 1 || ARGV.first&.start_with?("--")
  warn "usage: ruby scripts/check-branch-workflow-policy.rb [workflow.yml|--candidate-root DIR [--trusted-revision SHA --candidate-revision SHA]|--self-test]"
  exit 2
end

exit(check(ARGV.first || default_path) ? 0 : 1)
