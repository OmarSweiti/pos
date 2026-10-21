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

Screenshots never enter the code repository. They are committed to the evidence
repository — `<owner>/<repository>-evidence`, public and append-only — under
`pr-<n>/<UTC time>-<head>/`, in a folder of their own, so a clone of the code never
downloads them, and the comment embeds them at the evidence commit that stored
them, so a link can never be repointed. The script refuses unless the checkout
is the pull request's exact head with no tracked change, because the comment
names that commit as the one validated; it scans the evidence for secrets and
assistant attribution before storing anything, because the code repository's
pre-push hook never sees the evidence repository; and it verifies GitHub serves
every image before posting.

The engineering app posts the comment (`scripts/engineering-token.sh`), so the
`validation` check counts it, and the script then runs that check again for the
head so it reads the evidence now rather than at the next push.

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
import shutil
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
ROOT = Path(__file__).resolve().parent.parent
VALIDATION_WORKFLOW = "validation.yml"


def evidence_repository(repository: str) -> str:
    """The evidence repository beside a code repository: `<owner>/<name>-evidence`."""
    return f"{repository}-evidence"


def evidence_cache() -> Path:
    base = os.environ.get("XDG_CACHE_HOME") or str(Path.home() / ".cache")
    return Path(base) / "pos" / "evidence"


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
    store = evidence_repository(repository)
    folder = f"pr-{pr}/{run_directory}"
    lines = [
        f"{MARKER} pr={pr} head={head} evidence={evidence_commit} -->",
        f"## Validation evidence — `{head[:7]}`",
        "",
        f"**Validated commit:** `{head}` — the pull request's head when this was posted. "
        "A later push is not covered by this comment.",
    ]
    if evidence.images:
        lines.append(
            f"**Stored in:** [`{store}` @ `{evidence_commit[:7]}`]"
            f"(https://github.com/{store}/tree/{evidence_commit}/{folder})"
        )
    lines += ["", evidence.summary, ""]
    if evidence.images:
        lines += ["### Screenshots", ""]
        for image in evidence.images:
            url = raw_url(store, evidence_commit, f"{folder}/{image.name}")
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


def scan_evidence(directory: Path) -> None:
    """Refuse a secret or assistant attribution before anything leaves the machine:
    the evidence repository has no pre-push hook to catch either."""
    gitleaks = shutil.which("gitleaks")
    if gitleaks is None:
        raise Refusal("gitleaks is not installed, so the evidence cannot be scanned: run `just setup`")
    result = subprocess.run(
        [gitleaks, "dir", "--config", str(ROOT / ".gitleaks.toml"), "--redact=100",
         "--no-banner", "--log-level", "error", str(directory)],
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise Refusal(f"gitleaks finds a secret in {directory}; nothing was stored or posted")
    run(sys.executable, str(ROOT / "scripts" / "check-automation-attribution.py"),
        "--message-file", str(directory / SUMMARY))


# An installation token is GitHub's documented `ghs_` prefix and an opaque value.
# That value is not a fixed length or alphabet: today's tokens run to hundreds of
# characters and carry `.` and `-`, and a 40-character `[A-Za-z0-9_]` rule refused
# the first real one. So this proves only that the minting script printed one
# header-safe word of the right kind (not an error message, and not a personal
# token), and leaves the token's validity to GitHub.
APP_TOKEN = re.compile(r"ghs_[A-Za-z0-9._-]{16,4096}")


def app_token(repository: str) -> str:
    """A token for the engineering app, scoped to `repository`, which posts the
    comment so the `validation` check counts it."""
    result = subprocess.run(
        ["bash", str(ROOT / "scripts" / "engineering-token.sh"), repository],
        capture_output=True,
        check=False,
    )
    token = result.stdout.decode("utf-8", "replace").strip()
    if result.returncode != 0 or not APP_TOKEN.fullmatch(token):
        detail = result.stderr.decode("utf-8", "replace").strip()
        raise Refusal(
            "the engineering app posts the evidence, and it is not available "
            f"({detail or 'no token'}): run `just engineering-app`"
        )
    return token


def evidence_clone(store: str) -> Path:
    """This machine's clone of the evidence repository, current with GitHub."""
    clone = evidence_cache()
    url = f"git@github.com:{store}.git"
    if (clone / ".git").is_dir():
        remote = run("git", "-C", str(clone), "remote", "get-url", "origin")
        if remote != url:
            raise Refusal(f"{clone} is a clone of {remote}, not {url}")
        if run("git", "-C", str(clone), "status", "--porcelain"):
            raise Refusal(f"{clone} has local changes; the evidence store is append-only")
        run("git", "-C", str(clone), "pull", "-q", "--ff-only")
    else:
        clone.parent.mkdir(parents=True, exist_ok=True)
        run("git", "clone", "-q", url, str(clone))
    return clone


def store(pr: PullRequest, run_directory: str, evidence: Evidence) -> str:
    """Commit the evidence to a new folder of the evidence repository, push it,
    and return the commit that holds it."""
    clone = evidence_clone(evidence_repository(pr.repository))
    folder = f"pr-{pr.number}/{run_directory}"
    target = clone / folder
    if target.exists():
        raise Refusal(f"{folder} already exists in the evidence repository; rerun")
    target.mkdir(parents=True)
    for image in evidence.images:
        shutil.copyfile(image, target / image.name)
    (target / SUMMARY).write_text(evidence.summary + "\n", encoding="utf-8")
    name = run("git", "config", "user.name")
    email = run("git", "config", "user.email")
    try:
        run("git", "-C", str(clone), "add", "--", folder)
        run("git", "-C", str(clone), "-c", f"user.name={name}", "-c", f"user.email={email}",
            "commit", "-q", "-m", f"evidence: {pr.repository}#{pr.number} at {pr.head[:7]}")
        run("git", "-C", str(clone), "push", "-q", "origin", "HEAD")
    except Refusal:
        # Leave the clone as GitHub has it, so the next run starts clean.
        subprocess.run(["git", "-C", str(clone), "reset", "-q", "--hard", "@{upstream}"], check=False)
        subprocess.run(["git", "-C", str(clone), "clean", "-q", "-fd"], check=False)
        raise
    return run("git", "-C", str(clone), "rev-parse", "HEAD")


def rerun_validation(repository: str, head: str, token: str) -> None:
    """Run the `validation` check for `head` again, so it reads the evidence now."""
    env = {**os.environ, "GH_TOKEN": token}
    listed = subprocess.run(
        ["gh", "run", "list", "--repo", repository, "--workflow", VALIDATION_WORKFLOW,
         "--commit", head, "--limit", "1", "--json", "databaseId", "--jq", ".[0].databaseId // empty"],
        capture_output=True, env=env, check=False,
    )
    run_id = listed.stdout.decode("utf-8", "replace").strip()
    if listed.returncode != 0 or not run_id.isdigit():
        print(f"validation: no run for {head[:7]} yet; it reads the evidence when it starts")
        return
    # Let a run in progress finish first, then run it again.
    subprocess.run(["gh", "run", "watch", run_id, "--repo", repository],
                   capture_output=True, env=env, check=False)
    run("gh", "run", "rerun", run_id, "--repo", repository, env=env)
    time.sleep(5)
    watched = subprocess.run(
        ["gh", "run", "watch", run_id, "--repo", repository, "--exit-status"],
        capture_output=True, env=env, check=False,
    )
    if watched.returncode != 0:
        raise Refusal(
            f"the validation check still fails for {head[:7]} — read it: "
            f"gh run view {run_id} --repo {repository} --log"
        )
    print(f"validation: passed for {head[:7]}")


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


def check(number: int, directory: Path) -> tuple[PullRequest, Evidence, str]:
    evidence = load_evidence(directory)
    pr = pull_request(number)
    require_coverage(evidence, pr.runtime)
    require_checkout_at(pr.head)
    scan_evidence(directory)
    return pr, evidence, app_token(pr.repository)


def publish(number: int, directory: Path, *, dry_run: bool) -> None:
    pr, evidence, token = check(number, directory)
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
        commit = store(pr, run_directory, evidence)
        verify_served(
            evidence_repository(pr.repository), commit, f"pr-{pr.number}/{run_directory}", evidence
        )
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
        url = run(
            "gh", "pr", "comment", str(pr.number), "--body-file", handle.name,
            env={**os.environ, "GH_TOKEN": token},
        )
    finally:
        Path(handle.name).unlink(missing_ok=True)
    print(
        f"pr-evidence: posted {len(evidence.images)} screenshot(s) for {pr.head[:7]} — {url}"
    )
    rerun_validation(pr.repository, pr.head, token)


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
            "images are embedded from the evidence repository, at the commit that stored them",
            "https://raw.githubusercontent.com/owner/pos-evidence/"
            f"{commit}/pr-7/20261020T000000Z-aaaaaaa/01-sale.png" in body
            and "refs/evidence" not in body
            and "/main/" not in body,
        )
        expect(
            "the evidence repository sits beside the code repository",
            evidence_repository("owner/pos") == "owner/pos-evidence",
        )
        expect(
            "the comment links the stored folder at its commit",
            f"https://github.com/owner/pos-evidence/tree/{commit}/pr-7/20261020T000000Z-aaaaaaa"
            in body,
        )
        expect("the summary is carried verbatim", "Sale screen with two lines." in body)

    # Built, never literal, so no token-shaped string sits in the tree for a scanner.
    prefix = "ghs" + "_"
    expect("a classic 40-character app token is accepted",
           APP_TOKEN.fullmatch(prefix + "A1b2" * 9) is not None)
    expect("today's long app token, with `.` and `-`, is accepted",
           APP_TOKEN.fullmatch(prefix + "x" * 120 + "." + "y-z_" * 60 + "." + "Q" * 25)
           is not None)
    for label, output in (
        ("an empty output", ""),
        ("an error message", "Bad credentials"),
        ("two words", prefix + "A" * 36 + " extra"),
        ("two lines", prefix + "A" * 36 + "\n" + prefix + "B" * 36),
        ("a personal token", "ghp" + "_" + "A" * 36),
        ("a prefix with nothing after it", prefix),
    ):
        expect(f"{label} is not taken for an app token", APP_TOKEN.fullmatch(output) is None)

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
