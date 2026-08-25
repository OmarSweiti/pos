#!/usr/bin/env node
/** Run a repository Python hook without relying on shell-specific quoting.
 *
 * Claude Code invokes this launcher in exec form. Node is already a project
 * prerequisite and is a real executable on every supported platform. The
 * launcher selects the conventional Python command for the host, forwards the
 * hook payload unchanged on stdin, and preserves the hook's exit status.
 */

import { spawnSync } from "node:child_process";

const rawArgs = process.argv.slice(2);
const failClosed = rawArgs[0] === "--fail-closed";
if (failClosed) rawArgs.shift();
const [script, ...scriptArgs] = rawArgs;

const infrastructureFailure = (message) => {
  if (failClosed) {
    process.stderr.write(
      `BLOCKED: ${message} Configuration changes require a working policy hook.\n`,
    );
    process.exit(2);
  }
  process.stdout.write(`${JSON.stringify({ systemMessage: message })}\n`);
  process.exit(0);
};

if (!script) {
  infrastructureFailure("Claude hook launcher has no script path.");
}

let input;
try {
  input = await new Promise((resolve, reject) => {
    const chunks = [];
    process.stdin.on("data", (chunk) => chunks.push(chunk));
    process.stdin.on("end", () => resolve(Buffer.concat(chunks)));
    process.stdin.on("error", reject);
  });
} catch (error) {
  infrastructureFailure(`Claude hook launcher could not read its payload (${error.message}).`);
}

const candidates =
  process.platform === "win32"
    ? [
        ["py", ["-3"]],
        ["python3", []],
        ["python", []],
      ]
    : [
        ["python3", []],
        ["python", []],
      ];

for (const [command, prefix] of candidates) {
  const result = spawnSync(command, [...prefix, script, ...scriptArgs], {
    input,
    encoding: null,
    maxBuffer: 8 * 1024 * 1024,
  });

  if (result.error?.code === "ENOENT") continue;
  if (result.error) {
    infrastructureFailure(
      `Claude hook interpreter ${command} failed (${result.error.message}).`,
    );
  }
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  if (result.status === null) {
    infrastructureFailure(
      `Claude hook interpreter ${command} ended without an exit status.`,
    );
  }
  process.exit(result.status);
}

infrastructureFailure(
  "No Python 3 interpreter was found for the Claude hook. Git and CI guards remain active.",
);
