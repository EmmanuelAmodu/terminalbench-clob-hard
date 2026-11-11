# Simulator specification

Each invocation starts a fresh simulation. Worker loss clears its live order
state, progress, and local send cache. Committed journal records, checkpoints,
registered execution intents, confirmation records, accepted execution history,
and controller configuration survive worker loss.

## Trading and command outcomes

Match at the best price, then FIFO within that price. Fills use the resting
maker's price; unmatched incoming quantity rests at its limit price. New order
IDs must be positive u64 values and cannot be reused after acceptance, including
after cancellation or fill. Invalid new orders do not reserve IDs. Price and
positive quantity range from 1 through 1,000,000,000.

Amendments specify new remaining quantity. A reduction or unchanged quantity
retains priority; an increase moves behind existing orders at that price. Zero
amendment cancels. Price amendments are unsupported. priority_seq is the accepted
creation sequence or latest increasing-amendment sequence.

Request IDs are global numeric identities. An identical retry has no additional
command effect; a different payload under the same request ID is rejected.
Request identity excludes authority term and survives journal trimming.

New-order price, quantity, or zero-ID violations yield INVALID before checking
order reuse. Amendments above the quantity limit yield INVALID before checking
existence. Otherwise, canceling or amending any non-live ID, including zero,
yields UNKNOWN_ORDER. Reuse of an accepted new order ID yields DUPLICATE_ORDER.
These are committed outcomes without book mutation. Successful application yields
OK. Request conflicts, identical retries, stale senders, unavailable workers,
and submissions during catch-up do not append journal entries.

## Authority and recovery

The authoritative journal assigns consecutive command sequences from 1. The
initial worker is live in term 1. Promotion advances the term, fences every older
command/publication sender, restores the latest checkpoint, and makes the standby
current by sequential application of the retained committed tail. Repeated
promotion and replacement of a live worker are supported. Recovered order state,
reserved identities, priority, and logical fills agree with committed history.

Snapshots can be full or incremental and cover entries through their application
watermark inclusively. They require a live worker whose applied sequence equals
its application acknowledgement sequence. Delivery confirmation is independent.
A catch-up prefix may be checkpointed. The initial checkpoint is an empty book at
sequence 0. Compaction preserves the latest checkpoint's state and watermark.

Promotion can stop after restoration. A restoring worker is RECOVERING until its
applied sequence reaches the journal head. It cannot accept new commands while
catching up. Recovery may advance in bounded steps or lose the worker between
steps. Retrying application reconstructs identical logical execution content.
The outbox retains batches, including zero-fill batches, for applied commands;
registered intents remain available after worker loss or journal trimming.

Journal trimming removes retained command payloads through a chosen sequence.
The sequence must be at least the previous trim boundary and no greater than the
latest checkpoint watermark. Trimming preserves global request idempotency and
the committed sequence counter. Snapshot boundaries and pending execution intents
remain valid after trimming. Host-power-loss durability, consensus, and actual
network transport are outside this simulator.

## Publication and receipts

Execution identity is <command-sequence>.<zero-based-fill-ordinal>, with ordinals
assigned per maker fill in matching order. Identity and economics are unchanged
by authority term, delivery attempts, recovery steps, and compaction. The external
execution history exposes each logical fill once. Different economics under the
same execution identity are an error.

AUTO mode publishes and confirms eligible intents after normal submission and
after recovery reaches the journal head. DEFER mode registers intents without
automatic publication or confirmation. Switching mode alone does not flush work.

DRAIN selects eligible, unconfirmed intents in numeric (command sequence, ordinal)
order, excluding IDs already sent by the current worker. An intent is eligible
only when its command is applied by that worker. A new worker starts with an empty
send cache. Each physical delivery, including an identical retry, generates a
new transport receipt. An identical retry adds no execution-history record while
its deduplication entry is retained.

There are two delivery lanes, assigned by command sequence modulo 2. DRAIN and
CONFIRM may select one lane or both. CONFIRM consumes the oldest unconsumed
transport receipts in the selected lane(s). Logical confirmation of an execution
is idempotent. Confirmation is durable and does not depend on the live worker's
applied prefix. DRAIN can publish a covered prefix while recovery is incomplete.

COMPACT also advances a monotone delivery-retention floor. At each invocation,
its new value is the maximum of the previous floor and the largest consecutive
prefix of applied commands for which every logical execution is durably
confirmed. A zero-fill batch is complete. Deduplication keys at or below the floor
are reclaimed; keys above it remain. Accepted execution history, batch manifests,
confirmation identities, and transport receipt records remain available. The
RETENTION record reports the floor and current retained deduplication-key count.

## Input operations

Read UTF-8 from stdin, one operation per line, with whitespace-separated fields.
Blank lines and # comments are ignored. Numeric fields are decimal u64 values;
counts must also fit the platform's usize range. Side is B or S, lane is 0 or 1.
Malformed operations, numbers outside range, invalid sides/lanes, and violated
controller preconditions reject the script with exit 2 and stderr diagnostics.
Successful scripts exit 0.

| Operation | Effect |
| --- | --- |
| SUBMIT term request N id side price qty | Commit and apply a limit order |
| SUBMIT term request A id remaining_qty | Amend remaining quantity |
| SUBMIT term request C id | Cancel a live order |
| MODE AUTO / MODE DEFER | Set persistent publication configuration |
| SNAPSHOT / SNAPSHOT FULL | Save a full checkpoint at current acknowledged application |
| SNAPSHOT DELTA | Append an incremental checkpoint from the latest stored boundary |
| COMPACT | Materialize the checkpoint chain and perform delivery retention; requires a live worker |
| TRIM through | Trim committed payloads through a covered checkpoint boundary |
| CRASH | Destroy the worker and its volatile state |
| PROMOTE | Advance authority, restore, and finish recovery |
| PROMOTE PAUSE | Advance authority and restore without replay or automatic publication |
| RECOVER count | Apply up to count retained tail commands; requires a live worker |
| DRAIN term count [lane] | Publish up to count eligible intents in selected lane(s) |
| CONFIRM term count [lane] | Consume up to count receipt acknowledgements in selected lane(s) |
| STALE_PUBLISH term | Probe re-publication of the first visible execution |
| FAULT COMMIT / APPLY / ACK | Arm worker loss at the indicated next new-command stage |
| FAULT PUBLISH n | Arm worker loss after the nth physical send of the next DRAIN |
| FAULT RECOVER n | Arm worker loss after the nth applied command of the next recovery invocation |
| FAULT NONE | Clear all pending fault arms |

All sends and confirmations check authority before changing state. Fenced
operations yield notices; a current-term operation with no worker yields
UNAVAILABLE. STALE_PUBLISH requires an existing visible execution. It checks
fencing first, ignores a durably confirmed execution, and otherwise performs one
physical send of that execution. Term overflow rejects promotion.

Command-stage faults are one-shot, consumed only by a new append. COMMIT is
before application; APPLY is after book application and durable batch registration;
ACK is after the application acknowledgement, including in DEFER mode. Publication
and recovery faults are separate one-shot arms consumed by the next respective
invocation, including automatic invocations. A paused promotion does not consume
a recovery arm. A count larger than the work performed causes no loss. Counts may
be zero for DRAIN, CONFIRM, and RECOVER; armed fault n must be positive. Rejected
or fenced operations do not consume the corresponding arm.

## Output records

Stdout contains these records. Row ordering is not significant. Sequence fields,
fill ordinals, receipt positions, and queue contents carry the meaningful order.

| Record | Fields after record type |
| --- | --- |
| JOURNAL | seq term request followed by canonical N/A/C command, for retained entries |
| RESULT | seq outcome for applied entries, including trimmed entries |
| EXEC | stable_id seq ordinal maker taker aggressor_side maker_price qty |
| BOOK | id side price remaining_qty priority_seq for each live order |
| QUEUE | side price comma_separated_order_ids in FIFO order |
| STATUS | term committed_count applied acknowledged snapshot_seq live_flag |
| ROLE | LIVE, RECOVERING, or DEAD |
| CONFIG | AUTO or DEFER |
| TRIMMED | last_trimmed_command_sequence |
| BATCH | command_seq total_logical_executions confirmed_logical_executions |
| PENDING | stable_id for each registered, unconfirmed logical execution |
| RECEIPT | transport_position stable_id consumed_flag |
| RETENTION | retired_command_floor retained_deduplication_key_count |
| NOTICE | FENCED_COMMAND, FENCED_PUBLICATION, UNAVAILABLE, RECOVERING, DUPLICATE_REQUEST, or REQUEST_CONFLICT |

STATUS, ROLE, CONFIG, TRIMMED, and RETENTION occur once each. Transport positions
start at 1 and increase per physical send. Committed_count is the journal head,
not the number of retained payloads. A restoring worker has live_flag 1. A dead
worker has live_flag/applied/acknowledged 0 and no BOOK/QUEUE rows. Empty books
emit no BOOK/QUEUE rows. Durable outcomes, intents, receipt records, and accepted
execution history remain visible when the worker is absent. BATCH records remain
for all registered commands, including zero-fill and trimmed commands.
