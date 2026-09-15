# Octos UI Protocol Change Request: Scope Binding and accepted run_id

## Header

- Request id: `UPCR-2026-030`
- Title: Bind wire sessions to an authoritative Scope at entry; expose
  `run_id` on turn acceptance
- Date: 2026-09-15
- Target protocol: `octos-ui/v1alpha1`
- Status: proposed
- Depends on: none
- Spec: `specs/task-c1-cluster-scope-execution-context.spec.md`
- ADR: `docs/adr/cluster-state-and-execution.md` (D2)

## Summary

Cluster deployments need an unambiguous internal scope
(`Scope = tenant_id + profile_id + workspace_id + session_id`) that the
wire session ID alone cannot provide: two tenants may legitimately
present the same wire session ID, and a client-supplied cwd string is a
runner mapping location, never an identity.

This UPCR covers the protocol-visible edges of that binding:

1. **Scope binding semantics**: the server binds the authenticated
   identity + wire `session_id` to a `Scope` ONCE at entry. Downstream
   behavior (history lookup, ledger, approvals, broadcast, artifacts) is
   keyed by that Scope, not by the raw wire string. This is a server-side
   semantic clarification — the wire shape of `session_id` is unchanged.
2. **`turn/start` accepted payload gains an optional `run_id` field** so a
   client can correlate the accepted turn with the server's execution
   record (and later with recovery/replay).

## Motivation

Without an authoritative scope, multi-replica deployments cannot isolate
tenants that share wire session IDs, cannot replay events per-scope, and
cannot fence a recovered execution. `run_id` on the accept reply lets a
client distinguish "the server accepted my turn" from "a specific,
recoverable run owns it" — a prerequisite for resume-after-pod-loss UX.

## Wire shape

The existing `turn/start` accept result remains valid when `run_id` is
absent. A capable server may return:

```json
{ "accepted": true, "run_id": "run-0000000000000042-00000000000016fb" }
```

`run_id` is opaque. It is not a prompt body, credential, tool output, or
session key; it is the server-allocated execution identifier of the run
that owns the accepted turn. Clients that predate this field simply ignore
it (additive, backward compatible).

Scope binding itself has no new wire field: the client keeps sending the
same `session_id`; the server's binding decision (tenant/profile from the
authenticated identity, server-allocated `workspace_id`) is authoritative
and is NOT taken from any client-supplied workspace/cwd string.

## Negotiation and compatibility

- Feature name: `scope.binding.v1`.
- A server that does not implement this UPCR omits `run_id` and keeps the
  prior accept shape; clients must not require it.
- The binding change is server-internal and backward compatible on the
  wire; the only client-visible difference is the isolation guarantee
  (same wire `session_id` under different authenticated tenants no longer
  aliases one scope) and the additive `run_id`.

## Security and privacy

`run_id` discloses nothing beyond the existence of a run the caller
already caused. Scope binding closes a cross-tenant aliasing hole; it does
not widen any client's reach. The client cwd/workspace hint is accepted
only as a runner mapping location and never overrides the server-allocated
`workspace_id`.
