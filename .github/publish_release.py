"""Publish a draft release on Forgejo once every asset and signature is on it.

Usage: publish_release.py <tag> <notes file>. Env: FORGEJO_URL (default
https://git.serrebiradio.com), FORGEJO_REPO (owner/name, default
serrebi/BlindFB), FORGEJO_TOKEN. The runners have no `gh`, and `gh`
speaks GitHub's API anyway, so this talks to Forgejo's own release API.
It also writes SHA256SUMS over the four archives before publishing: no
package leg can make it (each sees only its own platform's files), and the
releases shipped without it had to be finished by hand.
"""

import hashlib
import json
import os
import sys
import urllib.request
import uuid

WANTED = [
    "BlindFB.zip", "BlindFB.zip.minisig",
    "BlindFB-Setup.exe", "BlindFB-Setup.exe.minisig",
    "BlindFB.tar.gz", "BlindFB.tar.gz.minisig",
    "BlindFB-arm64.dmg", "BlindFB-arm64.dmg.minisig",
]

# The order SHA256SUMS has listed the archives in since v0.7.48.
ARCHIVES = ["BlindFB.zip", "BlindFB-Setup.exe", "BlindFB.tar.gz", "BlindFB-arm64.dmg"]


API = os.environ.get("FORGEJO_URL", "https://git.serrebiradio.com").rstrip("/") + "/api/v1"


def call(method, path, body=None):
    request = urllib.request.Request(
        API + path,
        method=method,
        data=json.dumps(body).encode() if body is not None else None,
        headers={"Authorization": "token " + os.environ["FORGEJO_TOKEN"], "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request) as response:
        raw = response.read()
        return json.loads(raw) if raw else {}


def download(url):
    request = urllib.request.Request(url, headers={"Authorization": "token " + os.environ["FORGEJO_TOKEN"]})
    with urllib.request.urlopen(request) as response:
        return response.read()


def upload_asset(repo, release_id, data, name):
    boundary = "----blindfb" + uuid.uuid4().hex
    body = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="attachment"; filename="{name}"\r\n'
        "Content-Type: application/octet-stream\r\n\r\n"
    ).encode() + data + f"\r\n--{boundary}--\r\n".encode()
    request = urllib.request.Request(
        API + f"/repos/{repo}/releases/{release_id}/assets?name={name}",
        method="POST",
        data=body,
        headers={"Authorization": "token " + os.environ["FORGEJO_TOKEN"], "Content-Type": f"multipart/form-data; boundary={boundary}"},
    )
    with urllib.request.urlopen(request) as response:
        return json.load(response)


def add_checksums(repo, release):
    by_name = {a["name"]: a for a in release["assets"]}
    lines = []
    for name in ARCHIVES:
        digest = hashlib.sha256(download(by_name[name]["browser_download_url"])).hexdigest()
        lines.append(f"{digest}  {name}")
    if "SHA256SUMS" in by_name:
        call("DELETE", f"/repos/{repo}/releases/{release['id']}/assets/{by_name['SHA256SUMS']['id']}")
    upload_asset(repo, release["id"], ("\n".join(lines) + "\n").encode(), "SHA256SUMS")
    print("SHA256SUMS uploaded")


def main():
    tag, notes = sys.argv[1], open(sys.argv[2], encoding="utf-8").read()
    repo = os.environ.get("FORGEJO_REPO", "serrebi/BlindFB")
    # Fetching by tag skips drafts: find the draft in the list.
    release = next((r for r in call("GET", f"/repos/{repo}/releases?limit=50") if r["tag_name"] == tag), None)
    if release is None:
        sys.exit(f"no release for {tag}")
    present = {asset["name"] for asset in release["assets"]}
    print("\n".join(sorted(present)))
    missing = [name for name in WANTED if name not in present]
    if missing:
        sys.exit("missing: " + ", ".join(missing))
    # The job token cannot download a draft's files (404 on both download
    # routes; v0.7.68-v0.7.72 never published). A prerelease is public but
    # not /releases/latest, so no updater offers it before SHA256SUMS lands.
    release = call("PATCH", f"/repos/{repo}/releases/{release['id']}", {"draft": False, "prerelease": True})
    add_checksums(repo, release)
    call("PATCH", f"/repos/{repo}/releases/{release['id']}", {"body": notes, "draft": False, "prerelease": False})
    print(f"published {tag}")


if __name__ == "__main__":
    main()
