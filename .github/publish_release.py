"""Publish a draft release on Forgejo once every asset and signature is on it.

Usage: publish_release.py <tag> <notes file>. Env: GITHUB_API_URL (Forgejo's
/api/v1), GITHUB_REPOSITORY (owner/name), TOKEN. The runners have no `gh`, and
`gh` speaks GitHub's API anyway, so this talks to Forgejo's own release API.
"""

import json
import os
import sys
import urllib.request

WANTED = [
    "BlindFB.zip", "BlindFB.zip.minisig",
    "BlindFB.tar.gz", "BlindFB.tar.gz.minisig",
    "BlindFB-arm64.dmg", "BlindFB-arm64.dmg.minisig",
]


def call(method, path, body=None):
    request = urllib.request.Request(
        os.environ["GITHUB_API_URL"].rstrip("/") + path,
        method=method,
        data=json.dumps(body).encode() if body is not None else None,
        headers={"Authorization": "token " + os.environ["TOKEN"], "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request) as response:
        return json.load(response)


def main():
    tag, notes = sys.argv[1], open(sys.argv[2], encoding="utf-8").read()
    repo = os.environ["GITHUB_REPOSITORY"]
    # Fetching by tag skips drafts: find the draft in the list.
    release = next((r for r in call("GET", f"/repos/{repo}/releases?limit=50") if r["tag_name"] == tag), None)
    if release is None:
        sys.exit(f"no release for {tag}")
    present = {asset["name"] for asset in release["assets"]}
    print("\n".join(sorted(present)))
    missing = [name for name in WANTED if name not in present]
    if missing:
        sys.exit("missing: " + ", ".join(missing))
    call("PATCH", f"/repos/{repo}/releases/{release['id']}", {"body": notes, "draft": False, "prerelease": False})
    print(f"published {tag}")


if __name__ == "__main__":
    main()
