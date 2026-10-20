#!/usr/bin/env python3
"""Publish validation evidence for a pull request: screenshots, then one comment.

The owner approves every pull request (docs/implementation/03-github-workflow.md
§6) and approves it on evidence, not on a description. This script is the one
way that evidence reaches the pull request, so every run has the same shape.

    pr-evidence.py check   <pr> <directory>   # validate and preview; no writes
    pr-evidence.py publish <pr> <directory>   # push the images, post the comment
    pr-evidence.py --self-test

The directory holds `summary.md` and zero or more screenshots (`01-name.png`, at
most MAX_IMAGES, each at most MAX_IMAGE_BYTES — the repository's own blob cap,
so the pre-push hook accepts them). A pull request that changes what runs — code
under apps/, crates/ or packages/ that is not Markdown, or a dependency manifest,
lockfile or toolchain pin — must carry at least one screenshot: for a dependency
bump, a smoke run of the app is the evidence that it still works. A pull request
that changes nothing at runtime posts the summary alone, and the summary must say
`Runtime change: none` and why.

Screenshots never enter a branch. They are committed to `refs/evidence/pr-<n>`,
outside refs/heads, so `git clone` and the default fetch never download them,
and the comment embeds them by commit SHA, so a link can never be repointed.
Each run is one more commit on that ref, which keeps every earlier comment's
images intact. The script refuses unless the checkout is the pull request's
exact head with no tracked change, because the comment names that commit as
the one validated — and it verifies GitHub serves every image before posting.

The repository is public, so evidence is public forever. That is why the skill
(`.claude/skills/validate-pr`) has the operator inspect every image before
publishing: no real customer data, no PIN, card value, token or key.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path

MAX_IMAGES = 20
MAX_IMAGE_BYTES = 2_000_000
MAX_SUMMARY_BYTES = 20_000
SUMMARY = "summary.md"
IMAGE_NAME = re.compile(r"^[a-z0-9][a-z0-9._-]{0,80}\.(?:png|jpe?g)$")
RUNTIME_PATH = re.compile(r"^(?:apps|crates|packages)/")
# Not code, but they change the code that runs: a lockfile bump replaces a library.
RUNTIME_FILE = re.compile(
    r"^(?:.*/)?(?:Cargo\.lock|Cargo\.toml|package\.json|pnpm-lock\.yaml"
    r"|pnpm-workspace\.yaml|rust-toolchain\.toml)$"
)
NOT_APPLICABLE = "Runtime change: none"
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
JPEG_MAGIC = b"\xff\xd8\xff"
MARKER = "<!-- pos-validation-evidence"


class Refusal(Exception):
    """The evidence cannot be published as it stands."""


@dataclass(frozen=True)
class Evidence:
    summary: str
    images: tuple[Path, ...]


def load_evidence(directory: Path) -> Evidence:
    if not directory.is_dir():
        raise Refusal(f"{directory} is not a directory")
    summary_path = directory / SUMMARY
    if not summary_path.is_file():
        raise Refusal(
            f"{directory} has no {SUMMARY}: say what was validated, how, and the result"
        )
    raw_summary = summary_path.read_bytes()
    if len(raw_summary) > MAX_SUMMARY_BYTES:
        raise Refusal(
            f"{SUMMARY} is over {MAX_SUMMARY_BYTES} bytes; keep it to the evidence"
        )
    try:
        summary = raw_summary.decode("utf-8").strip()
    except UnicodeDecodeError as error:
        raise Refusal(f"{SUMMARY} is not UTF-8 ({error})") from error
    if not summary:
        raise Refusal(f"{SUMMARY} is empty")
    if MARKER in summary:
        raise Refusal(f"{SUMMARY} must not carry the evidence marker itself")

    images: list[Path] = []
    for entry in sorted(directory.iterdir()):
        if entry.name == SUMMARY:
            continue
        if entry.is_symlink() or not entry.is_file():
            raise Refusal(
                f"{entry.name}: only regular files belong in an evidence directory"
            )
        if not IMAGE_NAME.fullmatch(entry.name):
            raise Refusal(
                f"{entry.name}: name screenshots like 01-sale-screen.png "
                "(lowercase, digits, '.', '-', '_'; .png, .jpg or .jpeg)"
            )
        data = entry.read_bytes()
        if len(data) > MAX_IMAGE_BYTES:
            raise Refusal(
                f"{entry.name} is {len(data)} bytes; the cap is {MAX_IMAGE_BYTES}"
            )
        expected = PNG_MAGIC if entry.suffix == ".png" else JPEG_MAGIC
        if not data.startswith(expected):
            raise Refusal(f"{entry.name} is not the image type its extension claims")
        images.append(entry)
    if len(images) > MAX_IMAGES:
        raise Refusal(
            f"{len(images)} screenshots; the cap is {MAX_IMAGES} — keep the ones that prove a step"
        )
    return Evidence(summary=summary, images=tuple(images))


def is_runtime(paths: list[str]) -> bool:
    return any(
        (RUNTIME_PATH.match(path) and not path.endswith(".md"))
        or RUNTIME_FILE.match(path)
        for path in paths
    )


def require_coverage(evidence: Evidence, runtime: bool) -> None:
    if runtime and not evidence.images:
        raise Refusal(
            "this pull request changes runtime code, so it needs screenshots of the "
            "change working against the emulators"
        )
    if not evidence.images and NOT_APPLICABLE not in evidence.summary:
        raise Refusal(
            f"no screenshots: {SUMMARY} must say '{NOT_APPLICABLE}' and why, "
            "and list the checks that ran instead"
        )


def raw_url(repository: str, commit: str, path: str) -> str:
    return f"https://raw.githubusercontent.com/{repository}/{commit}/{path}"


def render_comment(
    *,
    pr: int,
    repository: str,
    head: str,
    evidence_commit: str,
    run_directory: str,
    evidence: Evidence,
) -> str:
    lines = [
        f"{MARKER} pr={pr} head={head} evidence={evidence_commit} -->",
        f"## Validation evidence — `{head[:7]}`",
        "",
        f"**Validated commit:** `{head}` — the pull request's head when this was posted. "
        "A later push is not covered by this comment.",
    ]
    if evidence.images:
        lines.append(
            f"**Archive:** `refs/evidence/pr-{pr}` @ "
            f"[`{evidence_commit[:7]}`](https://github.com/{repository}/tree/"
            f"{evidence_commit}/{run_directory})"
        )
    lines += ["", evidence.summary, ""]
    if evidence.images:
        lines += ["### Screenshots", ""]
        for image in evidence.images:
            url = raw_url(repository, evidence_commit, f"{run_directory}/{image.name}")
            lines += [f"**{image.name}**", "", f"![{image.name}]({url})", ""]
    return "\n".join(lines).rstrip() + "\n"


def run(
    *arguments: str, env: dict[str, str] | None = None, stdin: bytes | None = None
) -> str:
    result = subprocess.run(
        list(arguments),
        input=stdin,
        capture_output=True,
        env=env,
        check=False,
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).decode("utf-8", "replace").strip()
        raise Refusal(f"`{' '.join(arguments[:3])} …` failed: {detail}")
    return result.stdout.decode("utf-8").strip()


@dataclass(frozen=True)
class PullRequest:
    repository: str
    number: int
    head: str
    runtime: bool


def pull_request(number: int) -> PullRequest:
    repository = run(
        "gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"
    )
    if not re.fullmatch(r"[^/\s]+/[^/\s]+", repository):
        raise Refusal(f"GitHub returned an invalid repository name {repository!r}")
    data = json.loads(
        run("gh", "pr", "view", str(number), "--json", "state,headRefOid")
    )
    if data.get("state") != "OPEN":
        raise Refusal(f"#{number} is {data.get('state')}, not OPEN")
    head = data.get("headRefOid")
    if not isinstance(head, str) or not re.fullmatch(r"[0-9a-f]{40}", head):
        raise Refusal("GitHub returned an invalid head commit")
    files = run(
        "gh",
        "api",
        "--paginate",
        f"repos/{repository}/pulls/{number}/files",
        "--jq",
        ".[].filename",
    ).splitlines()
    return PullRequest(repository, number, head, is_runtime(files))


def require_checkout_at(head: str) -> None:
    local = run("git", "rev-parse", "HEAD")
    if local != head:
        raise Refusal(
            f"this checkout is at {local[:7]} but the pull request's head is {head[:7]}; "
            "validate the exact head, then publish"
        )
    if subprocess.run(["git", "diff", "--quiet", "HEAD"], check=False).returncode != 0:
        raise Refusal(
            "the checkout has tracked changes, so what ran is not the pull request's head"
        )


def build_commit(
    pr: PullRequest, run_directory: str, evidence: Evidence
) -> tuple[str, str | None]:
    ref = f"refs/evidence/pr-{pr.number}"
    remote = run("git", "ls-remote", "origin", ref)
    parent = remote.split()[0] if remote else None
    with tempfile.TemporaryDirectory(prefix="pos-evidence-") as temporary:
        env = {**os.environ, "GIT_INDEX_FILE": str(Path(temporary) / "index")}
        if parent:
            run("git", "fetch", "-q", "origin", ref)
            fetched = run("git", "rev-parse", "FETCH_HEAD")
            if fetched != parent:
                raise Refusal(f"{ref} moved while publishing; rerun")
            run("git", "read-tree", parent, env=env)
        else:
            run("git", "read-tree", "--empty", env=env)
        for path in (*evidence.images, None):
            name = path.name if path else SUMMARY
            data = path.read_bytes() if path else (evidence.summary + "\n").encode()
            blob = run("git", "hash-object", "-w", "--stdin", stdin=data)
            run(
                "git",
                "update-index",
                "--add",
                "--cacheinfo",
                f"100644,{blob},{run_directory}/{name}",
                env=env,
            )
        tree = run("git", "write-tree", env=env)
    message = (
        f"docs(repo): validation evidence for #{pr.number} at {pr.head[:7]}  [—]\n"
    )
    parents = ["-p", parent] if parent else []
    commit = run(
        "git", "commit-tree", tree, *parents, "-F", "-", stdin=message.encode()
    )
    return commit, parent


def verify_served(
    repository: str, commit: str, run_directory: str, evidence: Evidence
) -> None:
    for image in evidence.images:
        expected = hashlib.sha256(image.read_bytes()).hexdigest()
        # A distinct query string: a URL requested before its push lands is
        # negatively cached by the raw host, so never probe the URL the comment uses.
        url = (
            raw_url(repository, commit, f"{run_directory}/{image.name}")
            + f"?v={commit}"
        )
        for attempt in range(10):
            try:
                with urllib.request.urlopen(url, timeout=20) as response:  # noqa: S310 — fixed https host
                    if hashlib.sha256(response.read()).hexdigest() == expected:
                        break
            except urllib.error.URLError:
                pass
            time.sleep(3 * (attempt + 1))
        else:
            raise Refusal(
                f"GitHub does not serve {image.name} at {commit[:7]} yet; nothing was posted"
            )


def check(number: int, directory: Path) -> tuple[PullRequest, Evidence]:
    evidence = load_evidence(directory)
    pr = pull_request(number)
    require_coverage(evidence, pr.runtime)
    require_checkout_at(pr.head)
    return pr, evidence


def publish(number: int, directory: Path, *, dry_run: bool) -> None:
    pr, evidence = check(number, directory)
    run_directory = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + f"-{pr.head[:7]}"
    if dry_run:
        print(
            render_comment(
                pr=pr.number,
                repository=pr.repository,
                head=pr.head,
                evidence_commit="0" * 40,
                run_directory=run_directory,
                evidence=evidence,
            )
        )
        print(
            f"pr-evidence: check passed — {len(evidence.images)} screenshot(s); nothing was written"
        )
        return
    if evidence.images:
        commit, _parent = build_commit(pr, run_directory, evidence)
        run("git", "push", "origin", f"{commit}:refs/evidence/pr-{pr.number}")
        verify_served(pr.repository, commit, run_directory, evidence)
    else:
        commit = pr.head
    body = render_comment(
        pr=pr.number,
        repository=pr.repository,
        head=pr.head,
        evidence_commit=commit,
        run_directory=run_directory,
        evidence=evidence,
    )
    with tempfile.NamedTemporaryFile(
        "w", suffix=".md", encoding="utf-8", delete=False
    ) as handle:
        handle.write(body)
    try:
        url = run("gh", "pr", "comment", str(pr.number), "--body-file", handle.name)
    finally:
        Path(handle.name).unlink(missing_ok=True)
    print(
        f"pr-evidence: posted {len(evidence.images)} screenshot(s) for {pr.head[:7]} — {url}"
    )


def self_test() -> int:
    passed = failed = 0

    def expect(label: str, ok: bool) -> None:
        nonlocal passed, failed
        print(f"  {'ok  ' if ok else 'FAIL'}  {label}")
        passed += ok
        failed += not ok

    def refused(callable_, *arguments) -> bool:
        try:
            callable_(*arguments)
        except Refusal:
            return True
        return False

    png = PNG_MAGIC + b"\x00" * 16
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)

        def directory(name: str, files: dict[str, bytes]) -> Path:
            path = root / name
            path.mkdir()
            for file_name, data in files.items():
                (path / file_name).write_bytes(data)
            return path

        good = directory(
            "good", {SUMMARY: b"Sale screen with two lines.", "01-sale.png": png}
        )
        evidence = load_evidence(good)
        expect("a summary and a named screenshot load", len(evidence.images) == 1)
        expect(
            "a missing summary is refused",
            refused(load_evidence, directory("nosummary", {"01-a.png": png})),
        )
        expect(
            "an empty summary is refused",
            refused(load_evidence, directory("empty", {SUMMARY: b"  \n"})),
        )
        expect(
            "a summary forging the marker is refused",
            refused(
                load_evidence,
                directory("marker", {SUMMARY: f"{MARKER} x -->".encode()}),
            ),
        )
        expect(
            "a name with a space is refused",
            refused(
                load_evidence, directory("space", {SUMMARY: b"s", "01 sale.png": png})
            ),
        )
        expect(
            "an uppercase name is refused",
            refused(
                load_evidence, directory("upper", {SUMMARY: b"s", "01-Sale.png": png})
            ),
        )
        expect(
            "a non-image extension is refused",
            refused(
                load_evidence, directory("ext", {SUMMARY: b"s", "notes.txt": b"x"})
            ),
        )
        expect(
            "a PNG name over JPEG bytes is refused",
            refused(
                load_evidence,
                directory("magic", {SUMMARY: b"s", "01-a.png": JPEG_MAGIC + b"x"}),
            ),
        )
        expect(
            "an image over the blob cap is refused",
            refused(
                load_evidence,
                directory(
                    "big", {SUMMARY: b"s", "01-a.png": png + b"\x00" * MAX_IMAGE_BYTES}
                ),
            ),
        )
        many = {
            SUMMARY: b"s",
            **{f"{index:02d}-a.png": png for index in range(MAX_IMAGES + 1)},
        }
        expect(
            "more screenshots than the cap are refused",
            refused(load_evidence, directory("many", many)),
        )
        (root / "linked").mkdir()
        (root / "linked" / SUMMARY).write_bytes(b"s")
        (root / "linked" / "01-a.png").symlink_to(good / "01-sale.png")
        expect("a symbolic link is refused", refused(load_evidence, root / "linked"))

        summary_only = Evidence(summary="Docs only.", images=())
        not_applicable = Evidence(
            summary=f"{NOT_APPLICABLE} — the policy files only.", images=()
        )
        expect(
            "runtime code classifies as runtime",
            is_runtime(["docs/x.md", "apps/terminal/src/App.tsx"]),
        )
        expect(
            "crate source classifies as runtime",
            is_runtime(["crates/pos-domain/src/money.rs"]),
        )
        expect(
            "Markdown under apps/ is not runtime",
            not is_runtime(["apps/terminal/README.md"]),
        )
        expect(
            "policy and docs are not runtime",
            not is_runtime([".github/x.yml", "docs/a.md", "justfile"]),
        )
        expect(
            "a lockfile bump is runtime: it replaces a library",
            is_runtime(["Cargo.lock"]) and is_runtime(["pnpm-lock.yaml"]),
        )
        expect(
            "a manifest or toolchain pin is runtime",
            is_runtime(["apps/terminal/package.json"])
            and is_runtime(["rust-toolchain.toml"]),
        )
        expect(
            "a workflow or compose file is not runtime",
            not is_runtime([".github/workflows/ci.yml", "infra/docker-compose.yml"]),
        )
        expect(
            "a runtime PR without screenshots is refused",
            refused(require_coverage, summary_only, True),
        )
        expect(
            "a runtime PR cannot hide behind the N/A line",
            refused(require_coverage, not_applicable, True),
        )
        expect(
            "a non-runtime PR must say why there are no screenshots",
            refused(require_coverage, summary_only, False),
        )
        expect(
            "a non-runtime PR with the N/A line passes",
            not refused(require_coverage, not_applicable, False),
        )
        expect(
            "a runtime PR with screenshots passes",
            not refused(require_coverage, evidence, True),
        )

        head = "a" * 40
        commit = "b" * 40
        body = render_comment(
            pr=7,
            repository="owner/pos",
            head=head,
            evidence_commit=commit,
            run_directory="20261020T000000Z-aaaaaaa",
            evidence=evidence,
        )
        expect(
            "the comment carries a machine-readable marker naming PR, head and archive",
            body.startswith(f"{MARKER} pr=7 head={head} evidence={commit} -->"),
        )
        expect("the comment names the validated commit in full", f"`{head}`" in body)
        expect(
            "images are embedded by commit SHA, never by ref",
            f"https://raw.githubusercontent.com/owner/pos/{commit}/20261020T000000Z-aaaaaaa/01-sale.png"
            in body
            and "refs/evidence/pr-7/" not in body.split("### Screenshots")[1],
        )
        expect("the summary is carried verbatim", "Sale screen with two lines." in body)

    print(f"\npr-evidence self-test: {passed} passed, {failed} failed")
    return 0 if failed == 0 else 1


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        return self_test()
    parser = argparse.ArgumentParser(
        description="publish validation evidence for a pull request"
    )
    parser.add_argument("mode", choices=("check", "publish"))
    parser.add_argument("pr", type=int)
    parser.add_argument("directory", type=Path)
    arguments = parser.parse_args(argv)
    try:
        publish(arguments.pr, arguments.directory, dry_run=arguments.mode == "check")
    except Refusal as refusal:
        print(f"pr-evidence: REFUSED — {refusal}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
