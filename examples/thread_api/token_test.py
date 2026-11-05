#!/usr/bin/env python3
"""Sends messages to the two text endpoints of the frontdesk agent with three tokens, and
prints what each request gets.

Endpoints: "frontend" (public) and "backend" (public=False).
Tokens:    A = thread grant (all + five permissions), the token of main.rs
           B = endpoint call grant, the token of the console
           none = no Authorization header

For each endpoint and token, the script:
  new   sends a message without a contextId (this creates a thread)
  send  sends a message to an existing thread (THREAD_ID)
  read  calls tasks/list on that thread (reads its past turns)

Standard library only. Environment, the same as main.rs:
  LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET   required
  THREAD_ID   an existing AT_... thread; if not set, the first new thread is used
  AGENT       the agent name, default "dentist"
"""

import base64
import hashlib
import hmac
import json
import os
import sys
import time
import urllib.error
import urllib.request
import uuid

URL = os.environ["LIVEKIT_URL"].replace("ws", "http", 1).rstrip("/")
KEY = os.environ["LIVEKIT_API_KEY"]
SECRET = os.environ["LIVEKIT_API_SECRET"]
AGENT = os.environ.get("AGENT", "dentist")
QUESTION = "What do you know about me from this conversation? One sentence."


def b64(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode()


def sign(grants: dict) -> str:
    """An HS256 JWT signed with the project key, as livekit.agents.a2a.Credentials signs it."""
    now = int(time.time())
    claims = {"iss": KEY, "sub": "demo-user", "nbf": now - 5, "exp": now + 600, **grants}
    head = b64(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    body = b64(json.dumps(claims).encode())
    sig = hmac.new(SECRET.encode(), f"{head}.{body}".encode(), hashlib.sha256).digest()
    return f"{head}.{body}.{b64(sig)}"


FLAGS = ("list", "create", "write", "read", "delete")
TOKENS = {
    "A": sign({"agentThread": {"all": True, **{flag: True for flag in FLAGS}}}),
    "B": sign({"agentEndpoint": {"call": True}}),
    "none": None,
}


def rpc(endpoint: str, token: str | None, method: str, params: dict):
    """One JSON-RPC call. Returns (HTTP status or None, parsed body or error text)."""
    payload = {"jsonrpc": "2.0", "id": str(uuid.uuid4()), "method": method, "params": params}
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(
        f"{URL}/agents/{AGENT}/default/{endpoint}/v1",
        data=json.dumps(payload).encode(),
        headers=headers,
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            raw = response.read().decode(errors="replace")
            status = response.status
    except urllib.error.HTTPError as e:
        raw, status = e.read().decode(errors="replace"), e.code
    except (urllib.error.URLError, TimeoutError) as e:
        return None, str(e)
    try:
        return status, json.loads(raw)
    except json.JSONDecodeError:
        return status, raw.strip()[:200]


def texts(parts: list) -> str:
    return " ".join(part.get("text", "") for part in parts or []).strip()


def summary(status, body) -> str:
    if not isinstance(body, dict):
        return f"HTTP {status}  {body!r}"
    if "error" in body:
        error = body["error"]
        return f"HTTP {status}  rpc error {error.get('code')}: {error.get('message')}"
    result = body.get("result") or {}
    if "tasks" in result:
        tasks = result["tasks"]
        users = [texts(m.get("parts")) for t in tasks for m in t.get("history") or []]
        answers = [texts(a.get("parts")) for t in tasks for a in t.get("artifacts") or []]
        return (
            f"HTTP {status}  {len(tasks)} tasks readable, "
            f"first message {users[:1]!r}, first answer {[a[:60] for a in answers[:1]]!r}"
        )
    task = result.get("task", result)
    state = (task.get("status") or {}).get("state")
    answer = " ".join(texts(a.get("parts")) for a in task.get("artifacts") or [])
    return f"HTTP {status}  {state}  thread={task.get('contextId')}  answer={answer[:100]!r}"


def message(context_id: str | None) -> dict:
    out = {"messageId": str(uuid.uuid4()), "role": "user", "parts": [{"text": QUESTION}]}
    if context_id:
        out["contextId"] = context_id
    return {"message": out}


def main() -> None:
    thread_id = os.environ.get("THREAD_ID")
    print(f"{URL}/agents/{AGENT}/default/<endpoint>/v1\n")

    print("== new: message without contextId")
    for endpoint in ("frontend", "backend"):
        for name, token in TOKENS.items():
            status, body = rpc(endpoint, token, "message/send", message(None))
            print(f"{endpoint:<9} {name:<5} {summary(status, body)}")
            if thread_id is None and isinstance(body, dict):
                task = (body.get("result") or {}).get("task") or {}
                thread_id = task.get("contextId") or None

    if not thread_id:
        sys.exit("\nNo THREAD_ID and no new thread was created; set THREAD_ID=AT_...")

    print(f"\n== send: message to existing thread {thread_id}")
    for endpoint in ("frontend", "backend"):
        for name, token in TOKENS.items():
            status, body = rpc(endpoint, token, "message/send", message(thread_id))
            print(f"{endpoint:<9} {name:<5} {summary(status, body)}")

    print(f"\n== read: tasks/list on {thread_id}")
    params = {"contextId": thread_id, "historyLength": 10, "includeArtifacts": True}
    for endpoint in ("frontend", "backend"):
        for name, token in TOKENS.items():
            status, body = rpc(endpoint, token, "tasks/list", params)
            print(f"{endpoint:<9} {name:<5} {summary(status, body)}")


if __name__ == "__main__":
    main()
