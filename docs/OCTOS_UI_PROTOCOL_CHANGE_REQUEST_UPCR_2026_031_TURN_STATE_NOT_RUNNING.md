# Octos UI Protocol Change Request: Turn State "Not Running" Certainty

## Header

- Request id: `UPCR-2026-031`
- Date: 2026-09-18
- Target protocol: `octos-ui/v1alpha1`
- Status: implemented
- Scope: one additive optional result field on `turn/state/get` (UPCR-2026-011)

## Problem

A client that sends `turn/start` as the server dies (restart, crash) cannot
learn the turn's fate. The new process has no registry entry and no ledger
record for it, so `turn/state/get` answers `state: "unknown"` for ever. A
client that refuses to risk running a prompt twice then holds sending for ever,
because `unknown` does not distinguish "not running here" from "cannot tell".

Turns the ledger did record are already settled at boot by the restart sweep
(`turn/error` with code `orphaned_by_restart`); this request covers only the
unrecorded case.

## Contract

`TurnStateGetResult` gains one optional field:

- `running` (boolean): present as `false` only when the server is certain it is
  not executing the turn: the session is known, the active-turn registry holds
  no entry for the turn, the ledger projection has no lifecycle state for it,
  and no `turn/start` for that `(session_id, turn_id)` is being admitted.
  Absent in every other case, including whenever the server cannot be certain.

`state` keeps its UPCR-2026-011 meaning. `running: false` never accompanies
`active` or `interrupting`, and it makes no claim about whether the turn ever
ran or how it ended: an evicted, never-received, or lost turn all qualify.

Clients may treat `state: "unknown"` with `running: false` as proof that the
turn will not produce further output and settle it locally (never resending
it). Clients that ignore the field keep today's behaviour.

## Admission window

Between receipt of `turn/start` and the registry insert, a turn is in neither
source. The server tracks in-flight admissions keyed by the ids exactly as the
client sent them, and omits `running` while one is in progress, so a slow start
is never reported as not running.

## Tests

- `turn_state_get_returns_unknown_for_missing` (now also `running: false`)
- `should_not_claim_a_turn_is_stopped_while_its_start_is_still_being_admitted`
- `should_say_a_turn_is_not_running_once_its_admission_has_ended_without_a_record`
