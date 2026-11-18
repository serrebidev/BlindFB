"""Upload one package leg's dist/ files to the tag's draft release on Forgejo.

Usage: upload_release_assets.py <tag> <dist dir>. Env: FORGEJO_URL
(default https://git.serrebiradio.com), FORGEJO_REPO (owner/name,
default serrebi/BlindFB), FORGEJO_TOKEN.

Replaces softprops/action-gh-release, a GitHub-only action, with Forgejo's
own release API — the same move publish_release.py made for the Publish
job (014469a) and macos-release.yml makes with curl. The three package
legs race to create the draft, so creation retries against the release
list; a same-named asset is deleted before upload, so re-running a leg
replaces its files instead of duplicating or failing on them.
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request
import uuid

API = os.environ.get("FORGEJO_URL", "https://git.serrebiradio.com").rstrip("/") + "/api/v1"
REPO = os.environ.get("FORGEJO_REPO", "serrebi/BlindFB")
TOKEN = os.environ["FORGEJO_TOKEN"]


def call(method, path, body=None):
    request = urllib.request.Request(
        API + path,
        method=method,
        data=json.dumps(body).encode() if body is not None else None,
        headers={"Authorization": "token " + TOKEN, "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request) as response:
        raw = response.read()
        return json.loads(raw) if raw else {}


def find_release(tag):
    releases = call("GET", f"/repos/{REPO}/releases?limit=50")
    return next((r for r in releases if r["tag_name"] == tag), None)


def ensure_draft(tag):
    for _ in range(10):
        release = find_release(tag)
        if release is not None:
            return release
        try:
            return call("POST", f"/repos/{REPO}/releases", {"tag_name": tag, "draft": True})
        except urllib.error.HTTPError:
            # Another leg created it between the list and the create.
            time.sleep(3)
    sys.exit(f"could not create or find a draft release for {tag}")


def upload_asset(release_id, path, name):
    boundary = "----blindfb" + uuid.uuid4().hex
    data = open(path, "rb").read()
    body = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="attachment"; filename="{name}"\r\n'
        "Content-Type: application/octet-stream\r\n\r\n"
    ).encode() + data + f"\r\n--{boundary}--\r\n".encode()
    request = urllib.request.Request(
        API + f"/repos/{REPO}/releases/{release_id}/assets?name={name}",
        method="POST",
        data=body,
        headers={
            "Authorization": "token " + TOKEN,
            "Content-Type": f"multipart/form-data; boundary={boundary}",
        },
    )
    with urllib.request.urlopen(request) as response:
        return json.load(response)


def main():
    tag, dist = sys.argv[1], sys.argv[2]
    names = sorted(n for n in os.listdir(dist) if os.path.isfile(os.path.join(dist, n)))
    if not names:
        sys.exit(f"no files in {dist}")
    release = ensure_draft(tag)
    existing = {a["name"]: a["id"] for a in release["assets"]}
    for name in names:
        if name in existing:
            call("DELETE", f"/repos/{REPO}/releases/{release['id']}/assets/{existing[name]}")
        upload_asset(release["id"], os.path.join(dist, name), name)
        print(f"uploaded {name} to draft {tag}")


if __name__ == "__main__":
    main()
