#!/usr/bin/env python3
"""The engineering app: a GitHub App the owner owns, which opens the pull
requests prepared for the owner, so that the owner can review and approve them —
GitHub refuses an approval from a pull request's own author
(docs/implementation/03-github-workflow.md §6). Commits and branches stay the
owner's; the app only opens, assigns, asks for a review and comments.

    python3 scripts/engineering-app.py create    register the app from a manifest, then install it
    python3 scripts/engineering-app.py status    what is configured, and where it is installed

`create` serves one page on 127.0.0.1. It sends GitHub the app's definition,
GitHub asks the owner to confirm it (the name may be changed there), then hands
back the app's id and private key. The key is written to
~/.config/pos/engineering-app/private-key.pem (0600), outside every repository,
and never leaves this machine; the client secret is not kept, because nothing
uses it. Then install the app on the one repository the page names.

What the app may do, and nothing more: read code, metadata, checks and
statuses; write pull requests and issues (open, assign, request a review,
comment) and actions (run the validation check again). It has no webhook. It
cannot push code, change a setting or a ruleset, approve a review, or be
installed by anyone else. Standard library only.
"""

import http.server
import json
import os
import re
import secrets
import subprocess
import sys
import time
import urllib.parse
import urllib.request
import webbrowser

OWNER = "OmarSweiti"
REPOSITORIES = ["pos"]
HOME = os.environ.get("POS_ENGINEERING_APP_DIR") or os.path.join(
    os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config"),
    "pos",
    "engineering-app",
)
# The one-time code GitHub hands back after the app is registered.
CODE = re.compile(r"[A-Za-z0-9_-]{8,128}")
TOKEN_SCRIPT = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "engineering-token.sh"
)

PERMISSIONS = {
    "metadata": "read",
    "contents": "read",
    "checks": "read",
    "statuses": "read",
    "pull_requests": "write",  # open, request a review
    "issues": "write",  # assign, comment
    "actions": "write",  # run the validation check again
}


def manifest(redirect_url):
    return {
        "name": "POS Engineering",
        "url": "https://github.com/%s/pos" % OWNER,
        "description": "Opens the pull requests prepared for the point-of-sale project, so that "
        "its owner can review and approve them. It cannot push code or change settings.",
        "redirect_url": redirect_url,
        "public": False,
        "default_permissions": PERMISSIONS,
        "default_events": [],
    }


def page(title, body):
    return (
        "<!doctype html><meta charset=utf-8><title>%s</title>"
        "<body style='font:16px system-ui;max-width:40em;margin:4em auto'>%s</body>"
        % (title, body)
    ).encode()


def installed(repository):
    result = subprocess.run(
        ["bash", TOKEN_SCRIPT, "%s/%s" % (OWNER, repository)],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode == 0 and bool(result.stdout.strip())


def save(app):
    os.makedirs(HOME, mode=0o700, exist_ok=True)
    os.chmod(HOME, 0o700)
    key = os.path.join(HOME, "private-key.pem")
    descriptor = os.open(key, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w") as handle:
        handle.write(app["pem"])
    record = {k: app[k] for k in ("id", "slug", "name", "html_url", "created_at")}
    record["owner"] = app["owner"]["login"]
    with open(os.path.join(HOME, "app.json"), "w") as handle:
        json.dump(record, handle, indent=2)
        handle.write("\n")
    return record


def create():
    if os.path.exists(os.path.join(HOME, "app.json")):
        print("engineering-app: the engineering app is already configured in %s" % HOME)
        return status()
    state = secrets.token_urlsafe(24)
    created = {}

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args):  # the query carries a one-time code: never log it
            pass

        def reply(self, html):
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(html)

        def do_GET(self):  # noqa: N802 — the name http.server dispatches to
            url = urllib.parse.urlparse(self.path)
            if url.path == "/":
                action = "https://github.com/settings/apps/new?state=" + urllib.parse.quote(state)
                definition = json.dumps(
                    manifest("http://127.0.0.1:%d/created" % self.server.server_port)
                )
                self.reply(
                    page(
                        "Create the engineering app",
                        (
                            "<h1>Create the engineering app</h1><p>GitHub will show the app's "
                            "definition: read access to code, write access to pull requests, "
                            "issues and actions — nothing else. Confirm it there.</p>"
                            "<form method=post action='%s'><input type=hidden name=manifest value='%s'>"
                            "<button>Continue to GitHub</button></form>"
                            "<script>document.forms[0].submit()</script>"
                        )
                        % (action, definition.replace("&", "&amp;").replace("'", "&#39;")),
                    )
                )
            elif url.path == "/created":
                query = urllib.parse.parse_qs(url.query)
                code = query.get("code", [""])[0]
                # Only the answer to this request, and only a code of GitHub's shape:
                # it becomes one path segment of a fixed api.github.com URL, nothing else.
                if query.get("state", [""])[0] != state or not CODE.fullmatch(code):
                    self.reply(page("Refused", "<p>This answer did not come from the request made here.</p>"))
                    return
                request = urllib.request.Request(
                    "https://api.github.com/app-manifests/%s/conversions"
                    % urllib.parse.quote(code, safe=""),
                    method="POST",
                    headers={"Accept": "application/vnd.github+json"},
                )
                with urllib.request.urlopen(request, timeout=30) as response:  # noqa: S310 — fixed https host
                    created.update(save(json.load(response)))
                install = "https://github.com/apps/%s/installations/new" % created["slug"]
                self.reply(
                    page(
                        "Created",
                        (
                            "<h1>%s is created</h1><p>Now install it on this repository only:</p>"
                            "<ul>%s</ul><p><a href='%s'>Install it</a></p>"
                        )
                        % (
                            created["name"],
                            "".join("<li>%s/%s</li>" % (OWNER, r) for r in REPOSITORIES),
                            install,
                        ),
                    )
                )
            else:
                self.send_response(404)
                self.end_headers()

    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    server.timeout = 5
    start = "http://127.0.0.1:%d/" % server.server_port
    print("engineering-app: open %s in your browser (opening it now) and confirm the app on GitHub" % start)
    webbrowser.open(start)
    deadline = time.time() + 30 * 60
    while not created and time.time() < deadline:
        server.handle_request()
    server.server_close()
    if not created:
        print("engineering-app: no app was created within 30 minutes", file=sys.stderr)
        return 1
    print(
        "engineering-app: created %s (app id %s); the key is in %s"
        % (created["name"], created["id"], HOME)
    )
    install = "https://github.com/apps/%s/installations/new" % created["slug"]
    print("engineering-app: install it on %s — %s" % (", ".join(REPOSITORIES), install))
    webbrowser.open(install)
    missing = REPOSITORIES
    while time.time() < deadline:
        missing = [r for r in REPOSITORIES if not installed(r)]
        if not missing:
            print("engineering-app: installed on %s" % ", ".join(REPOSITORIES))
            return 0
        time.sleep(10)
    print("engineering-app: still not installed on %s" % ", ".join(missing), file=sys.stderr)
    return 1


def status():
    path = os.path.join(HOME, "app.json")
    if not os.path.exists(path):
        print("engineering-app: no engineering app configured — run: just engineering-app")
        return 3
    with open(path) as handle:
        app = json.load(handle)
    print(
        "engineering-app: %s (%s[bot], app id %s), owned by %s"
        % (app["name"], app["slug"], app["id"], app["owner"])
    )
    missing = []
    for repository in REPOSITORIES:
        ok = installed(repository)
        print("  %s  %s/%s" % ("installed    " if ok else "NOT INSTALLED", OWNER, repository))
        if not ok:
            missing.append(repository)
    if missing:
        print("engineering-app: install it — https://github.com/apps/%s/installations/new" % app["slug"])
    return 1 if missing else 0


def main(argv):
    sys.stdout.reconfigure(line_buffering=True)  # progress shows as it happens, even through a pipe
    commands = {"create": create, "status": status}
    if len(argv) != 1 or argv[0] not in commands:
        print(__doc__.split("\n\n")[1])
        return 2
    return commands[argv[0]]()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
