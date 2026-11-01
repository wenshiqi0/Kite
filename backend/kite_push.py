"""FCM HTTP v1 transport. Credentials and tokens stay on the private host.

Input/output are bounded JSON on stdio. No user message, prompt or tool body is sent.
Kite Node owns subscriptions, event ordering, retries and the persistent outbox.
"""
import base64
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

TOKEN_URL = "https://oauth2.googleapis.com/token"
SCOPE = "https://www.googleapis.com/auth/firebase.messaging"


def private_json(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077 or info.st_size > 65536:
            raise ValueError("private credential/cache file required")
        return json.loads(stream.read(65537))


def post(url, body, headers):
    request = urllib.request.Request(url, data=body, headers=headers, method="POST")
    with urllib.request.urlopen(request, timeout=8) as response:
        raw = response.read(65537)
        if len(raw) > 65536:
            raise ValueError("provider response too large")
        return json.loads(raw)


def access_token(account, cache):
    now = int(time.time())
    try:
        saved = private_json(cache)
        if saved["email"] == account["client_email"] and saved["expires"] > now + 60:
            return saved["token"]
    except (OSError, ValueError, KeyError, TypeError):
        pass
    encode = lambda data: base64.urlsafe_b64encode(data).rstrip(b"=")
    header = encode(json.dumps({"alg": "RS256", "typ": "JWT"}, separators=(",", ":")).encode())
    claims = encode(json.dumps({"iss": account["client_email"], "scope": SCOPE,
                               "aud": TOKEN_URL, "iat": now, "exp": now + 3600}, separators=(",", ":")).encode())
    unsigned = header + b"." + claims
    with tempfile.TemporaryDirectory(prefix="kite-push-key-") as temporary:
        key = Path(temporary) / "key.pem"
        key.write_text(account["private_key"])
        key.chmod(0o600)
        signature = subprocess.run(["openssl", "dgst", "-sha256", "-sign", str(key)], input=unsigned,
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=5, check=True).stdout
    assertion = (unsigned + b"." + encode(signature)).decode()
    response = post(TOKEN_URL, urllib.parse.urlencode({"grant_type": "urn:ietf:params:oauth:grant-type:jwt-bearer",
                                                      "assertion": assertion}).encode(),
                    {"Content-Type": "application/x-www-form-urlencoded"})
    token = response["access_token"]
    if not isinstance(token, str) or not 1 <= len(token) <= 8192:
        raise ValueError("invalid access token")
    pending = Path(str(cache) + ".pending")
    with pending.open("w", opener=lambda path, flags: os.open(path, flags | os.O_NOFOLLOW, 0o600)) as stream:
        json.dump({"email": account["client_email"], "token": token,
                   "expires": now + min(3600, int(response["expires_in"]))}, stream)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(pending, cache)
    return token


def payload(subscription, event):
    if not re.fullmatch(r"[a-f0-9]{64}", event["id"]) or event["kind"] not in ("completed", "attention", "question", "failed"):
        raise ValueError("invalid event")
    if not re.fullmatch(r"[a-zA-Z0-9_-]{1,128}", subscription["host_id"]):
        raise ValueError("invalid host identity")
    return {"message": {"token": subscription["token"], "data": {
        "kite_notice_id": subscription["host_id"] + "/" + event["id"],
        "kite_kind": event["kind"], "kite_session_id": event["session_id"], "kite_created_ms": str(event["created_ms"])},
        # Data-only high priority: the native service posts the same privacy-preserving
        # notification used by SSH, with one persistent deduplication identity.
        "android": {"priority": "HIGH", "ttl": "3600s", "restricted_package_name": "io.github.wenshiqi0.kite"}}}


def deliver(account_file, cache, job):
    account = private_json(account_file)
    project = account["project_id"]
    if not re.fullmatch(r"[a-z][a-z0-9-]{4,62}", project) or job["subscription"]["project_id"] != project:
        raise ValueError("Firebase project mismatch")
    if len(job["events"]) > 16:
        raise ValueError("push batch too large")
    token = access_token(account, cache)
    results = []
    for event in job["events"]:
        try:
            post("https://fcm.googleapis.com/v1/projects/" + project + "/messages:send",
                 json.dumps(payload(job["subscription"], event)).encode(),
                 {"Content-Type": "application/json", "Authorization": "Bearer " + token})
            status = "sent"
        except urllib.error.HTTPError as error:
            # Never return provider error bodies: they can contain tokens or credentials.
            raw = error.read(65536)
            try:
                details = json.loads(raw).get("error", {}).get("details", [])
                removed = any(row.get("errorCode") == "UNREGISTERED" for row in details)
            except (ValueError, TypeError, AttributeError):
                removed = False
            status = "unregistered" if removed else "retry"
        except (OSError, ValueError):
            status = "retry"
        results.append({"id": event["id"], "status": status})
        if status != "sent":
            break
    return {"results": results}


def main():
    os.umask(0o077)
    try:
        if len(sys.argv) != 3:
            raise ValueError("credential and private cache paths required")
        raw = sys.stdin.buffer.read(65537)
        if len(raw) > 65536:
            raise ValueError("push job too large")
        print(json.dumps(deliver(sys.argv[1], sys.argv[2], json.loads(raw))), flush=True)
    except Exception:
        print('{"results":[],"problem":"push transport unavailable; check host credentials, project and network"}', flush=True)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
