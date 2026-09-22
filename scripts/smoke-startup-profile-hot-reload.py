#!/usr/bin/env python3
"""Smoke: startup-pinned profile LLM hot-reload on k8s octos serve.

Sequence (all against profile `admin` — eager-loaded into state.profiles at
serve startup, i.e. startup-pinned; its profile file lives on the writable
PVC, unlike the read-only ConfigMap-mounted cluster-worker):

1. profile/llm/list  — capture the current primary (must stay untouched).
2. profile/llm/upsert — add a FALLBACK model (set_primary=false, so the
   serving primary never changes), assert applied=true,
   restart_required=false, runtime_disposition=reloaded.
3. profile/llm/delete — remove that fallback, assert the same triple.
4. Exit non-zero on any assertion failure.
"""
import asyncio
import json
import sys

import websockets

ENDPOINT = "ws://127.0.0.1:50080/api/ui-protocol/ws"
TOKEN = "test-cluster-token-12345"
PROFILE = "admin"
FAMILY = "openai"
MODEL = "gpt-4o-mini"


async def rpc(ws, req_id, method, params):
    await ws.send(json.dumps({
        "jsonrpc": "2.0",
        "id": req_id,
        "method": method,
        "params": params,
    }))
    while True:
        frame = json.loads(await ws.recv())
        # Skip notifications; wait for the matching response id.
        if frame.get("id") != req_id:
            continue
        if "error" in frame and frame["error"] is not None:
            raise AssertionError(f"{method} rpc error: {json.dumps(frame['error'])[:400]}")
        return frame.get("result", {})


def check_transition(result, label):
    assert result.get("applied") is True, f"{label}: applied!=true: {json.dumps(result)[:400]}"
    assert result.get("restart_required") is False, (
        f"{label}: restart_required!=false (hot reload broken): {json.dumps(result)[:400]}"
    )
    assert result.get("runtime_disposition") == "reloaded", (
        f"{label}: disposition={result.get('runtime_disposition')!r} != reloaded: {json.dumps(result)[:400]}"
    )


async def main():
    async with websockets.connect(
        ENDPOINT,
        additional_headers={"Authorization": f"Bearer {TOKEN}"},
        max_size=8 * 1024 * 1024,
    ) as ws:
        listed = await rpc(ws, "s1", "profile/llm/list", {"profile_id": PROFILE})
        primary = (listed.get("primary") or {})
        print(f"[1] primary before: {primary.get('family_id')}/{primary.get('model_id')}")
        assert primary.get("model_id"), f"no primary configured: {json.dumps(listed)[:400]}"

        upserted = await rpc(ws, "s2", "profile/llm/upsert", {
            "profile_id": PROFILE,
            "selection": {
                "family_id": FAMILY,
                "model_id": MODEL,
                "route": {
                    "route_id": "official",
                    "api_key_env": "OPENAI_API_KEY",
                    "api_type": "openai",
                },
            },
            "set_primary": False,
        })
        check_transition(upserted, "upsert-fallback")
        print(f"[2] upsert fallback: disposition=reloaded restart_required=false "
              f"revision={upserted.get('config_revision')}")
        # The serving primary must be untouched by a fallback upsert.
        stamp = upserted.get("runtime_policy_stamp") or {}
        assert stamp.get("model") == primary.get("model_id"), (
            f"stamp model changed unexpectedly: {stamp.get('model')!r} != {primary.get('model_id')!r}"
        )
        print(f"[3] stamp model still {stamp.get('model')} (primary untouched)")

        deleted = await rpc(ws, "s4", "profile/llm/delete", {
            "profile_id": PROFILE,
            "family_id": FAMILY,
            "model_id": MODEL,
            "route_id": "official",
        })
        check_transition(deleted, "delete-fallback")
        print("[4] delete fallback: disposition=reloaded restart_required=false")

    print("SMOKE PASS: startup-pinned profile hot-reloads without restart")


asyncio.run(main())
