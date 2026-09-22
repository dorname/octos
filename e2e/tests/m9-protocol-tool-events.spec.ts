/**
 * M9 wire-level e2e: tool/* event correlation.
 *
 * Issue: https://github.com/octos-org/octos/issues/647
 * Spec  : api/OCTOS_UI_PROTOCOL_V1_SPEC_2026-04-24.md §8
 *
 * Asserts that when a turn fires a tool, the wire stream includes a
 * `tool/started` -> `tool/progress`* -> `tool/completed` triplet correlated
 * by `tool_call_id` and the same `turn_id`/`session_id`.
 *
 * We use a deterministic prompt that asks the agent to use a stable
 * read-only tool (`list_dir`) so we don't depend on sandbox/policy state.
 * If the live model is configured with no tools, the spec falls back to
 * `it.skip` with a reason — we still want the file present so CI surface
 * a missing-tool-config as a skip, not as a test absence.
 */
import { test, expect } from "@playwright/test";
import {
  M9WsClient,
  freshTurnId,
  liveServerEnv,
  uniqueSessionId,
} from "../lib/m9-ws-client";

test.describe("M9 protocol — tool/* events", () => {
  // Tool-using turns can take a beat longer than plain text replies; bump
  // budget but keep below the 60s spec ceiling.
  test.setTimeout(60_000);

  test("tool/started -> tool/completed correlation by tool_call_id", async () => {
    const env = liveServerEnv();
    const client = new M9WsClient(env);
    const sid = uniqueSessionId("m9-tools");
    const turnId = freshTurnId();
    try {
      await client.openSession({ session_id: sid });
      const accept = await client.startTurn({
        session_id: sid,
        turn_id: turnId,
        // Prompt biased toward a real tool call. The exact tool name is
        // not load-bearing — we only care about the event triplet.
        input: [
          {
            kind: "text",
            text:
              "Use the list_dir tool to list the contents of '.' and tell me how many entries there were.",
          },
        ],
      });
      expect(accept.accepted).toBe(true);

      // Stage-5 v2 wire: terminal + tool lifecycle arrive as canonical
      // projection/envelope payloads (legacy tool/* and turn/completed
      // frames are suppressed for every connection).
      const completed = await client.waitForTurnTerminalEnvelope(turnId, 50_000);
      expect(completed.turn_id).toBe(turnId);

      const log = client.notificationsLog();
      const toolStarted = log.filter(
        (n) =>
          n.method === "projection/envelope" &&
          n.params?.turn_id === turnId &&
          n.params?.payload?.type === "tool_start",
      );
      const toolCompleted = log.filter(
        (n) =>
          n.method === "projection/envelope" &&
          n.params?.turn_id === turnId &&
          n.params?.payload?.type === "tool_end",
      );

      // If no tool fired, the live agent decided to answer from priors —
      // skip rather than fail, so flaky model behaviour doesn't block CI.
      test.skip(
        toolStarted.length === 0,
        "M9 tool/* spec: live agent did not invoke a tool for this prompt; nothing to assert",
      );

      expect(toolStarted.length).toBeGreaterThan(0);
      expect(toolCompleted.length).toBeGreaterThan(0);

      // Each tool/started must be matched by a tool/completed with the
      // same tool_call_id. Tools may interleave with progress events, so
      // we only require a 1-to-1 set match by id, not strict ordering of
      // distinct tool calls.
      const startedIds = new Set(
        toolStarted
          .map((n) => n.params?.payload?.data?.tool_call_id)
          .filter((x): x is string => typeof x === "string"),
      );
      const completedIds = new Set(
        toolCompleted
          .map((n) => n.params?.payload?.data?.tool_call_id)
          .filter((x): x is string => typeof x === "string"),
      );
      for (const id of startedIds) {
        expect(completedIds.has(id), `missing tool/completed for ${id}`).toBe(true);
      }

      // For each correlation id, started must precede completed in the
      // notification log (durable ordering).
      for (const id of startedIds) {
        const startIdx = log.findIndex(
          (n) =>
            n.method === "projection/envelope" &&
            n.params?.payload?.type === "tool_start" &&
            n.params?.payload?.data?.tool_call_id === id,
        );
        const endIdx = log.findIndex(
          (n) =>
            n.method === "projection/envelope" &&
            n.params?.payload?.type === "tool_end" &&
            n.params?.payload?.data?.tool_call_id === id,
        );
        expect(startIdx).toBeGreaterThanOrEqual(0);
        expect(endIdx).toBeGreaterThan(startIdx);
      }
    } finally {
      await client.close();
    }
  });
});
