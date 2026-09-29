# Workflow Node Guide

English | [中文](workflow-node-guide.zh.md)

A node-by-node usage guide for Ora workflow authors: what every canvas node is for, how to configure it, which variables it exposes, how it behaves at run time, and how to combine nodes into real scenarios. Every behavior described here matches the current code and is backed by two layers of hands-on verification: real runs of two sample workflows on the installed Ora 0.3.0 (including one genuine loop-limit failure), and the full 22-scenario production test suite passing on latest main.

Architecture and data models (tables, engine internals, full recovery semantics) are not repeated here — see the [Workflow architecture doc](workflow.md). This guide is about how to use the nodes, why they behave the way they do, and what to do when things go wrong.

## Contents

1. [Quick start](#1-quick-start)
2. [Core concepts](#2-core-concepts)
3. [Node reference](#3-node-reference)
   - [Start](#31-start)
   - [Agent](#32-agent)
   - [Condition](#33-condition)
   - [Variable Aggregator](#34-variable-aggregator)
   - [Iteration](#35-iteration)
   - [Loop and Exit loop](#36-loop-and-exit-loop)
   - [Output](#37-output)
4. [Running and observing](#4-running-and-observing)
5. [Sample walkthroughs](#5-sample-walkthroughs)
6. [Scenario recipes](#6-scenario-recipes)
7. [Best practices and pitfalls](#7-best-practices-and-pitfalls)
8. [Appendix: reference tables](#8-appendix-reference-tables)

## 1. Quick start

Your first workflow in five minutes:

1. **Open the editor.** Click “Workflows” (marked Alpha) in the sidebar; the main area becomes the canvas.
2. **Create a workflow.** `Ctrl/Cmd+N` or the list `+` menu → New workflow. The fresh draft already contains a Start node.
3. **Add nodes.** The bottom dock lists the available nodes in execution order: Start → Agent → Condition → Variable Aggregator → Iteration → Loop → Output. Click to add centered on the canvas, or press and drag to a position.
4. **Connect.** Drag from a node's right port to the next node's left port. Edges are execution order; the canvas is a directed acyclic graph scheduled in topological order.
5. **Configure and publish.** Click a node to configure it in the right inspector (the draft autosaves); click “Publish” to freeze the draft into an immutable version and make it the active version.
6. **Run.** From the `+` menu of a project or task row → “Run workflow”, pick a published workflow, fill in the start inputs, and start. The run freezes the active snapshot at that moment; later draft edits do not affect a started run.

> **Tip**: on the Ora 0.3.0 install we tested, the workflow library already contains two ready-to-run samples — “Iterate over a requirements list” and “Loop-check a code snippet”. See the step-by-step teardown in [Section 5](#5-sample-walkthroughs).

## 2. Core concepts

### 2.1 Canvas, nodes, and edges

- **Nodes** are units of execution; **edges** define ordering. The engine topologically sorts from the Start node along the edges; a node becomes ready only after all predecessors complete, and ready nodes in the same wave run in parallel.
- **Only the subgraph reachable from Start executes.** You can keep spare nodes and unconnected groups on the canvas — they never enter scheduling, variable pools, or sessions; the editor and run Overview mark them “Excluded from execution”. Reconnecting a node restores normal execution.
- Edges cannot form cycles (the graph must be acyclic); edges across container boundaries (Iteration regions / Loop bodies) are rejected.
- The canvas supports pointer/hand modes, box selection, wheel zoom, a minimap, “Organize nodes” auto-layout, annotation notes, and session-scoped undo/redo (cleared when you leave the editor; autosave is unaffected).

### 2.2 Drafts and versions

Every workflow has exactly one **draft** (the editable workspace, autosaved) and any number of **published versions** (immutable snapshots):

| Operation       | Behavior                                                                                                                                                                                     |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Publish         | Copies the current draft into a new snapshot and immediately makes it the **active version**; new runs use it                                                                                |
| Version history | Preview any published version (read-only); “Restore to this version” copies that graph back into the draft without changing the active version                                               |
| Set as active   | Switches the active pointer to a historical version and syncs its graph into the draft                                                                                                       |
| Delete version  | Removes a single published version; the draft and the current active version cannot be deleted                                                                                               |
| Import / export | `.reactflow.json` files; import previews before persisting, pinpoints parse failures, and only warns about missing plugins; export records plugin IDs and enabled flags only — never secrets |

A live run freezes its snapshot: a version referenced by a run cannot be deleted, and a workflow with active runs cannot be deleted.

### 2.3 Variables

Variables are the only channel for passing data between nodes.

**Selectors and template syntax.** A variable is addressed by node ID + variable name (+ optional nested path). Reference it in text with `{{#node.variable#}}`:

```text
Current requirement: {{#iteration-1.item#}}
Previous round report: {{#loop-review.report#}}
Source code: {{#start.source#}}
Object field: {{#iter.item.note#}}            ← take a field of an array[object] element
Structured field: {{#agent-1.structured_output.body#}}
```

**Where variables come from.** The “Insert variable” menu in the inspector groups them by source:

- **Global variables** (below);
- variables produced by **upstream nodes along the execution path** — not only direct predecessors but earlier ancestors too (Condition nodes are transparent: downstream nodes see the Condition's predecessors' variables; the Condition itself produces none);
- **scope isolation rules** for containers:
  - Members inside an Iteration region see the `item`/`index` round bindings, variables upstream of the iteration node, and in-region upstream products; they **never** see the iteration's own `output`/`entries`/`failed_count`.
  - Outer consumers see the iteration's three exposed variables; they **never** see region-member products or `item`/`index`.
  - Members inside a Loop body can reference carried variables (`{{#loopId.variable#}}`), variables visible outside the loop boundary, and in-body upstream products.

> **Tip (known editor-menu limitation)**: the scope rules above are the runtime truth, but the editor's “Insert variable” menu currently lists only globals and in-body products inside a Loop body — carried and outer variables must be hand-typed (exactly what the default loop template and the two samples do). The same applies to a Loop's named exports: the menu shows a generic `{loopId}.output` entry the runtime does not accept; reference loop outputs by their named export (e.g. `{{#loop-review.final_report#}}`).

**Value types.** Variables are statically typed, enforced identically at graph parsing, editor entry, and pool writes. The full type table is in [Appendix A](#appendix-a-value-types).

**Global and system variables.** The “Global variables” dialog manages them:

- System variables `sys.workflow_id` (current workflow ID, string) and `sys.timestamp` (the run-creation timestamp, number; re-seeded when execution resumes after an app restart) are runtime-provided and read-only;
- Custom globals require a **dotted name** (e.g. `cfg.threshold`), an explicit type, and a type-correct initial value; once saved they are visible everywhere without edges.

**The file type.** `file` is a durable workspace-relative reference shaped `{"kind": "workspace_file", "path": "relative/path"}`; `array[file]` is an array of those. Absolute paths, `..` traversal, empty paths, and platform-reserved names are rejected.

**Runtime validation.** A template reference must be **declared and assigned** at render time; otherwise the node fails with `prompt_template` (“prompt template cannot be rendered”) — never a silent blank. Referencing a sibling branch's variable from a parallel node is therefore dangerous: topological order does not guarantee the sibling has finished.

### 2.4 Node overview

| Node                | Execution form   | In one line                                                  | Where it can live                                   | Exposed variables                                                                                     |
| ------------------- | ---------------- | ------------------------------------------------------------ | --------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Start               | Swift            | Define workflow inputs and kickoff instruction               | Root (exactly one per graph)                        | `{id}.input` + each declared input                                                                    |
| Agent               | Async (session)  | Delegate one step of work to a model                         | Root / Iteration region / Loop body                 | `{id}.output` (final answer text); with structured output also `{id}.structured_output` + field paths |
| Condition           | Swift            | Route execution by rules (IF/ELIF/ELSE)                      | Root / Iteration region / Loop body                 | None (passes predecessors' variables through)                                                         |
| Variable Aggregator | Swift            | Collapse mutually exclusive branch outputs into one variable | Root / Iteration region                             | `{id}.output` (candidates' common type)                                                               |
| Iteration           | Composite region | Run the region once per array element                        | Root                                                | Outer: `{id}.output`, `{id}.entries`, `{id}.failed_count`; in-region: `{id}.item`, `{id}.index`       |
| Loop                | Composite region | Repeat the body until the end condition holds                | Root                                                | `{id}.{carriedName}` (in body), `{id}.{outputName}` (output bindings)                                 |
| Exit loop           | Swift            | End the owning Loop early                                    | Loop body only (from a member output-port `+` menu) | None                                                                                                  |
| Output              | Swift            | Assemble the final result                                    | Root / Loop body                                    | None (consumes variables)                                                                             |

“Swift” nodes complete synchronously inside one scheduling wave; “Async” nodes drive a real Agent session; “Composite” nodes own a region subgraph and manage rounds themselves. Prototype kinds (Tool, Merge, Human confirmation, Subflow) stay out of the dock until their runtime lands.

## 3. Node reference

### 3.1 Start

The single entry point of a workflow; it cannot be deleted, and the dock disables Start while one exists.

**Configuration:**

- **Initial prompt**: the workflow's default kickoff instruction. If a run supplies no kickoff input, the run inherits this text, which also lands in the `{start}.input` variable.
- **Input variables**: strongly typed values collected before a run starts. Each variable has:

| Field                     | Meaning                                                                                                                         |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Variable name             | No dots, unique per graph; runtime errors name this (not the display label)                                                     |
| Display name              | Field label on the start-input form (optional)                                                                                  |
| Value type                | See [Appendix A](#appendix-a-value-types)                                                                                       |
| Field type (form control) | Text / Paragraph / Select / Number / Checkbox / Single file / File list / JSON — decides which control the start screen renders |
| Initial value             | Optional; leave empty to fill at deployment. A select's initial value must be one of its options                                |
| Required                  | When checked, the start screen demands a value                                                                                  |
| Max length                | string/secret only, a positive integer; bounds both the editor default and run inputs                                           |

**Control-to-type mapping:**

| Control          | Value type                                                                                                                                                                                           |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Text / Paragraph | string                                                                                                                                                                                               |
| Select           | string (limited to the configured options)                                                                                                                                                           |
| Number           | number                                                                                                                                                                                               |
| Checkbox         | boolean                                                                                                                                                                                              |
| Single file      | file                                                                                                                                                                                                 |
| File list        | array[file]                                                                                                                                                                                          |
| JSON             | object / any / array / array[string] / array[number] / array[object] / array[boolean] / array[any] (`array[file]` cannot be declared); the initial value and run inputs must match the declared type |

**Typical use.** Text/paragraph for free-form input; JSON + `array[object]` for the object arrays that feed iterations; Select to pin enumeration values (preventing typos at run time); single file / file list for workspace files.

**Boundary validation (observed behavior).** Start-time rejections surface as the `workflow_run_input_invalid` public error naming the **variable and the reason**; one submission reports at most the **first refused value in alphabetical order** and applies no value from it — every save points at exactly one field to fix:

- a value that fails its declared type (e.g. a string for number, an array for object);
- an unknown variable name;
- a select value outside its options (“value is not one of the configured options”);
- a missing required value (“required value is missing”);
- an unsafe file path (absolute path, `..` traversal).

### 3.2 Agent

The only node that does work: it spawns a real Agent session (a dedicated CLI Agent process) that executes the prompt's task inside the run's chosen workspace.

**Configuration:**

- **Agent model**: pick the executor (Agent CLI) and model, e.g. `OpenCode · Sonnet`.
- **Role**: optional. The resolved role is injected into the session as a behavioral constraint; “No role” injects nothing.
- **Custom prompt**: the task-instruction template. Supports “Insert variable”; predecessor outputs are **never appended implicitly** — they reach the prompt only when you reference them (e.g. `{{#agent-1.output#}}`).
- **Required skills**: declared skills are installed into the workspace and force-invoked at the start of the node's prompt. This is a “must call” contract, not security isolation — other materialized skills in the workspace remain visible to the Agent. Enabling a skill that does not exist is rejected **when the run is created** (`workflow_skill_not_found`).
- **Allowed MCPs**: pick and enable from installed MCP plugins. **Only MCPs enabled here are authorized for this node's session** (a strict allowlist); selecting none means the node uses no MCP servers; an enabled but missing/unavailable plugin fails the node explicitly — never a silent skip. Ordinary chats are unaffected.
- **Interactive mode**: the node parks at “awaiting input” after its first turn and you keep chatting inside the node session (human in the loop). Agents inside Iteration regions and Loop bodies cannot enable it (the editor disables the switch there).
- **Structured output**: configure an object-rooted JSON Schema (visual field editor or raw Schema editor — both edit one dialog-local draft that is saved as a whole). It constrains **only the final reply** — the Agent may still reason, call tools, and edit files, but its last assistant message must be one bare JSON object. On success the parsed object lands in `{id}.structured_output` (field paths expand in selectors); on failure the node fails with `structured_output`, while `{id}.output` still keeps the raw answer text for debugging.
- **Retry on failure** (added after 0.3.0): a switch plus Max retries (0–5) and First wait in seconds (0–300). Unset means the default policy: enabled, up to 2 retries, first wait 10 s. Waits double exponentially, capped at 600 s. **Only these failures retry**: agent session failures, session ended without a stop reason, session binding rejected, structured output invalid, agent refusal, unknown stop reason. Retries do not roll back file changes; a retried attempt is told why the previous one failed (for reply-class failures); interactive nodes never retry; every other kind fails at once, and the failure block says “this kind of failure is not retried automatically”.

**Exposed variables:** `{id}.output` (string, the final assistant text — without the injected workflow-context blocks) and, when enabled, `{id}.structured_output` (object) plus field paths.

**Run-output standing:** if no Output node completes, a completed Agent can act as the run's final output fallback.

### 3.3 Condition

Routes execution by rules: IF (plus any number of ELIF) and a default ELSE branch.

**Configuration:** each branch (case) has:

- **Branch logic**: “All of the following” (AND) or “Any of the following” (OR);
- **Rule list**: variable (from the visible ones) × operator × value. Negation is expressed by picking the complementary operator (e.g. “not equals”, “not contains”).

**Evaluation semantics (observed behavior):**

- Branches evaluate in declaration order; **the first match wins**. When two branches overlap, the earlier one wins — the scenario suite pins exactly this.
- If nothing matches, the **ELSE branch** is selected; if no default branch is connected, nothing after the Condition executes.
- A branch participates only with at least one rule; a rule-less branch (you have not clicked “Add condition” yet) falls through — it never matches vacuously and steals the ELSE.
- **Unset variables**: `empty`/`not_empty` tolerate unassigned (unassigned counts as empty); every other operator fails the node with `condition_evaluation` (“condition cannot be evaluated”) rather than guessing a branch.
- **Type mismatches** fail the same way instead of yielding false — e.g. “greater than” on a string.

**Operators:** the UI offers 12 (equals, not equals, contains, not contains, starts with, ends with, greater than, less than, greater than or equal, less than or equal, empty, not empty); semantics in [Appendix B](#appendix-b-comparison-operators). The engine additionally accepts `is`/`is_not` (boolean equality) and `exists`/`not_exists` (assigned-ness), mainly for imported graphs and Loop end conditions.

**Transparency:** a Condition produces no variables; its downstream sees the Condition's **predecessors'** variables — including through a chain of Conditions. The branch choice is private scheduler state, never a variable.

### 3.4 Variable Aggregator

Collapses the outputs of **mutually exclusive branches** into one variable so the rejoined downstream references a single name.

**Configuration:**

- **Candidate variables**: an ordered list of branch-produced variables (e.g. `branch-a.output`, `branch-b.output`). **Declaration order is the priority**; reorder with move up/down.

**Semantics:**

- At run time the **first assigned** candidate passes its pool value through unchanged as `{id}.output`. “Assigned” means the key exists in the pool — **not a truthiness check**: `false`, `0`, `""`, `[]`, `{}`, and `null` all count as assigned and hit.
- All candidates must share one type (parse-time check); `{id}.output` is declared with that common type.
- Candidates must be produced by the aggregator's static transitive predecessors (mutually exclusive sibling branches qualify) or by globals.
- **No candidate assigned → the node fails** (stable `aggregator_no_match`). The aggregator is a pass-through multiplexer, not a default-value provider — if you need a fallback, make every branch assign.

**When not to use it:** if the branches are not mutually exclusive (both may run), the aggregator picks “whoever finished first”, not “whoever was planned”. For parallel joins, route the branches to different downstream nodes instead, or aggregate with separate Output nodes.

### 3.5 Iteration

Runs the region subgraph **once per element** of an array source — foreach semantics. It is an embedded composite region on the canvas: members are added only through the fixed internal start at the region's left, insertion on internal edges, and the append menu on unconnected member outputs; outer nodes cannot be dropped into a region, and members cannot leave it.

**Configuration:**

| Field           | Meaning                                                                                                                             |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Iterator source | An array-typed variable (e.g. `start.requirements`, an agent's structured array field)                                              |
| Collect target  | A root variable declared inside the region (usually the terminal member's `{member}.output`); each round takes its value            |
| Error strategy  | `fail` (default): the first failed round fails the run; `continue`: failed rounds are recorded in the ledger and the rest still run |
| Max iterations  | Default 50, minimum 1; **a source longer than this fails the node at the startup boundary** — never a silent truncation             |

**Round bindings and products:**

- Every round injects `{iterationId}.item` (the element, typed by the array's element type) and `{iterationId}.index` (0-based). Reference them directly in in-region prompts, e.g. `{{#iteration-1.item#}}`; elements of `array[object]` also resolve nested paths (`{{#iter.item.note#}}`).
- On completion the node exposes three variables (fixed types regardless of error strategy):

| Variable                     | Type          | Meaning                                                                        |
| ---------------------------- | ------------- | ------------------------------------------------------------------------------ |
| `{iterationId}.output`       | array[T]      | the collect target's per-round values; T is the collect target's declared type |
| `{iterationId}.entries`      | array[object] | per-round ledger `{item, status, output, error}` aligned with the source array |
| `{iterationId}.failed_count` | number        | number of failed rounds                                                        |

**Boundary behavior (observed behavior):**

- **Empty array**: legal. Zero rounds, zero sessions, immediate completion with empty outputs.
- **Over the ceiling**: fails at start, with the element count, the ceiling, and guidance in the error.
- **Collect target bypassed**: under `continue`, a round whose branch skipped the collect target settles as a **failed round** (“collect target did not run this round”) instead of reading the previous round's stale value; `{id}.output` contains only the rounds that actually ran.
- A region cannot contain Output nodes or nested composites (Iteration/Loop); member out-edges must stay inside; no outer node may target a member.
- Region agents cannot be interactive (new members default off; legacy interactive members show a “turn off interactive mode” repair action, and publish is rejected until fixed).
- Every round is its own execution scope: the same definition node has independent node-run rows and sessions per round; in-region Condition decisions are recorded per round.

**Editing:** regions collapse/expand (display only), resize by dragging the corner (20 px grid, minimum 560×340), and lay out independently under “Organize nodes”. Deleting a non-empty region confirms the member count and cascades members and incident edges (undoable); deleting the collect-target node clears that selector and asks for a replacement. Run views group region members by `(node, round)` with a round selector for any round's session and status.

**Recovery semantics:** the Iteration node is the resume unit — any failure inside clears all rounds and their products and **reruns from round 1**; in-round resume is out of scope.

### 3.6 Loop and Exit loop

A feedback container that **repeats until a condition holds**. Versus Iteration: iteration is “do this once per element” (rounds driven by the array), a loop is “feed each round's result into the next round” (rounds driven by the end condition).

Adding a Loop node creates a valid container group automatically: **the Loop node + a child Start (“Round start”) + a child Agent (“Loop Agent”)** with a default contract — carried variable `value` starting empty, each round feeding the child Agent's output back into `value`, stopping when the child Agent's output is not empty, and exporting the child Agent's output as `result`.

**Loop configuration (loopConfig):**

| Field                 | Meaning                                                                                                                                                                                                               |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Max rounds            | 1–100. Reaching the ceiling without the end condition matching **fails the Loop**                                                                                                                                     |
| Carried variables     | Typed cross-round values: name + type + initial (constant, or a selector evaluated at entry) + feedback selector (whose value is written back after each round). Reference in-body as `{{#loopId.name#}}`             |
| End condition (until) | AND/OR rule groups over child outputs (including structured fields), carried variables, and variables visible outside the loop boundary. **Evaluated after each round**: match → Loop succeeds; no match → next round |
| Outputs               | Variable bindings exported when the Loop succeeds (name + selector), exposed as `{loopId}.{outputName}`                                                                                                               |

> **Note**: a Loop exposes only the named exports from its Outputs list (`{loopId}.{outputName}`, e.g. `loop-review.final_report`). The editor's variable menu currently shows a generic `{loopId}.output` entry for Loop nodes — the runtime does not accept it (the engine assigns only named exports), so type the export name by hand; the 5.2 sample's Output node does exactly that.

**Round semantics (observed behavior):**

- Each round is a durable scope with per-round child rows; the Theater Loop inspector switches rounds to inspect each round's child status and session.
- The end condition is evaluated **after each round**, not mid-round; the first round may already satisfy it and succeed immediately.
- **Ceiling failure**: hitting the max rounds without a match fails the Loop with “Loop did not terminate within N rounds” (see the real case in [5.2](#52-loop-check-a-code-snippet-loop-a-real-loop-ceiling-case)). This is deliberate — it prevents a “always almost there” loop from silently burning its round budget.
- Emptiness/existence checks (`empty`/`not_empty`) tolerate unassigned variables; other comparisons fail on unassigned instead of **reusing a previous round's value** — a broken feedback chain fails loudly.
- Output binding selectors must be assigned at settlement time, or the Loop fails.

**Body rules:** the body is a separate DAG with exactly one reachable child Start; it may contain Agent, Condition, Output, and Exit loop; **no nesting** of Loops or Iterations; edges cannot cross the boundary; carried variables and boundary-visible outer variables are usable inside.

**Exit loop.** Add it from a loop member's output-port `+` menu; it belongs to that Loop, accepts incoming edges, and has no output port. Reaching an exit **ends the owning Loop early** (not the outer workflow): the scheduler prioritizes a ready exit node, cancels still-running siblings in that round, freezes outputs from the committed round, and publishes them. The classic pattern is `Condition → Exit loop` for “stop as soon as a business state is reached”. You may delete all end conditions and steer the Loop purely with exit nodes; the round ceiling still applies as the backstop.

**Recovery semantics:** the Loop node is the resume unit — any body failure (including an abandoned retry wait) clears every round and its products and **reruns from round 1**; “roll back only the failed node's files” is unavailable for this unit (`composite_region`), while “roll back to checkpoint” restores the worktree from before the loop.

### 3.7 Output

The terminal node: assembles several variables into the run's final result object. It must be terminal (no outgoing edges).

**Configuration: the Output bindings list** — each entry is “result name + result variable” (e.g. `review_report` ← `loop-review.final_report`). Result names are scoped to their own node; Output nodes on separate branches may reuse names, each building its own object.

**Run final-output precedence:**

1. A completed Output node's result object is the run's final output; if several Output nodes complete (separate branches each reaching their own terminal), the most recently finished one wins;
2. otherwise a completed Agent or Iteration acts as the fallback (the most recently finished wins a tie as well);
3. control nodes — Condition, Aggregator, Exit loop, child Starts — never contribute the run output.

An Output inside a Loop body exports that round's view of the bindings; the more common pattern is an Output outside the Loop referencing the Loop's exported variables (as the 5.2 sample does).

## 4. Running and observing

### 4.1 Starting a run

From the `+` menu of a **project row** or **task row** → “Run workflow” (only published workflows are listed). The run executes in that row's workspace: a project row targets the Main Workspace; a Task row targets that task's isolated workspace. The backend infers no branch and creates no extra worktree.

The start dialog collects:

- **Run name** (defaults to the workflow name);
- **Kickoff input (optional)**: this run's instruction overriding the Start node's initial prompt; it lands in `{start}.input`;
- **The start-variable form**: each Start-declared input rendered by its control, with required and type validation per [3.1](#31-start).

On “Start”: the run freezes the active snapshot, initializes the variable pool, and begins scheduling. Start variables left without an initial value are filled exactly here. Sessions bound to node runs never appear in the ordinary chat list; sessions of terminal nodes are read-only.

### 4.2 Run views: Theater and Overview

- **Theater**: a focused act stage + the execution-path rail on the left + a read-only act inspector (configuration, errors, artifacts) on the right. Parallel nodes switch via chips or arrow keys; the stage card embeds a **session dock** — expand it to read the full conversation with chat bubbles and Markdown rendering, with thoughts and tool calls behind a collapsed disclosure. Loop runs offer round switching and round history; Iterations offer the region round selector.
- **Overview**: renders the whole published snapshot as a canvas with per-node status coloring, zoom and pan; iteration members group by `(node, round)`, and clicking a node shows that round's details.

### 4.3 Node and run statuses

| Status           | Meaning                                                                                                 |
| ---------------- | ------------------------------------------------------------------------------------------------------- |
| Pending          | Not started (no node-run row)                                                                           |
| Running          | Executing                                                                                               |
| Awaiting input   | An interactive Agent parked for human input; the sidebar surfaces that human action is needed           |
| Waiting to retry | The wait window of an automatic retry (orange, with countdown and attempt number); nothing is executing |
| Inactive branch  | Downstream nodes on the unselected side of a Condition; this run never executes them                    |
| Succeeded        | Completed                                                                                               |
| Failed           | Failed (the run fails with it, but in-flight siblings run to completion and stay bindable)              |
| Cancelled        | Cancelled                                                                                               |

Failure stops dispatch (D2 semantics): any root-scope node failure fails the run immediately and the scheduler dispatches nothing new; already-dispatched nodes run to their terminal state. Every committed run or node-run transition publishes an invalidation event; the frontend re-queries on a loss-tolerant poll (~1.5 s).

### 4.4 Failures, retries, and recovery

**Failure detail.** The failed node's inspector shows the failure kind (18 kinds, [Appendix C](#appendix-c-failure-kinds)), message, underlying error chain, and attempt number; below it, **earlier failed attempts** are listed oldest-first, each tagged with what replaced it (automatic retry, manual resume, or run-again-from-start). Every node takes a pre-node git checkpoint (`refs/ora/checkpoints/<node_run_id>`) in the worktree and records its file-change list.

**Three recovery entry points:**

1. **Resume from failure**: soft-deletes the failed node runs and their descendants, then reschedules from the surviving state. The dialog offers rollback modes:
   - Keep as-is (default): leave the worktree alone;
   - Roll back only the failed node's files: restore the paths the failed nodes recorded;
   - Roll back to checkpoint: restore the whole worktree to before the resume unit ran (a `pre-rollback-*` snapshot is taken first so you can undo).
   - When the failure is inside an Iteration region or Loop body, the resume unit is the whole container and rounds restart from 1; “roll back files” is unavailable for containers.
2. **Run again from start (restart)**: rotates the execution identity and reruns the whole run; old history is retained but no longer affects scheduling.
3. **Resume on a newer published snapshot**: a failed run may switch to a newer published version; the engine checks graph compatibility (removed nodes, changed node types, Start-contract changes, and variable type changes are incompatible, with specific reasons shown).

**Previous-failure injection.** On by default: the previous failed attempt of the same node execution (for reply-class failures) is injected into the new attempt's prompt to tell the Agent why it failed — structured-output misses and refusals are the classic beneficiaries. A run-level switch turns it off.

## 5. Sample walkthroughs

The two samples below come from the workflow library of the Ora 0.3.0 install we tested and run as-is. We verified both with real runs: one complete success, one genuine loop-ceiling failure — both teardowns included.

### 5.1 Iterate over a requirements list (Iteration, the success case)

**Goal**: hand each of three requirements to an Agent, and collect what was done for each.

**Graph:**

```text
┌────────┐   ┌──────────────────────────────────┐   ┌────────┐
│ Start  │──▶│ Iteration: one requirement each  │──▶│ Output │
│        │   │  ┌────────────────────────────┐  │   │        │
│ source │   │  │ (internal start) ─▶ Agent  │  │   │results │
│ array  │   │  └────────────────────────────┘  │   │        │
└────────┘   └──────────────────────────────────┘   └────────┘
```

**Key configuration:**

- Start: input variable `requirements` of type `array[string]`, defaulting to three Chinese requirement strings;
- Iteration: iterator source `start.requirements`; collect target `process-1.output`; error strategy `fail`; max rounds 3;
- the in-region Agent prompt (excerpt):

  ```text
  Fulfill exactly one requirement from the requirements list.

  Current round index (0-based): {{#iteration-1.index#}}
  Current requirement: {{#iteration-1.item#}}

  Complete this requirement and answer in one sentence: what changed + how it was verified.
  ```

- Output: `results` ← `iteration-1.output`.

**Real run result** (verified on the installed app): all three rounds succeeded, each with its own Agent session, and the Agent left real file changes in the run workspace (each round with a git checkpoint and a change record): round 1 created `server.py` (+113 lines) and `tests/test_health.py` (+125), round 2 extended `server.py` and added `tests/test_upload.py` (+150), round 3 extended `README.md` (+33). The final output was a three-element array:

```json
{
  "results": [
    "Change: /health did not exist in the repo, created server.py with a stdlib-only implementation …",
    "Change: added POST /upload with a size cap to server.py …",
    "Change: added a “Local setup” section to README.md …"
  ]
}
```

Takeaways: round bindings render **inside each round's prompt**; `index` counts from 0; the collect target aggregates per round; the Output node references the aggregated array in one binding.

### 5.2 Loop-check a code snippet (Loop, a real loop-ceiling case)

**Goal**: review the same snippet repeatedly, carrying each round's report into the next, until a report confirms no remaining issues (contains PASS), with at most 3 rounds.

**Graph:**

```text
┌────────┐   ┌──────────────────────────────────────────┐   ┌──────────┐
│ Start  │──▶│ Loop: review and fix                     │──▶│ Output   │
│        │   │  ┌──────────┐     ┌───────────────┐      │   │          │
│ source │   │  │ Round    │────▶│ Code reviewer │      │   │review_   │
│ string │   │  │ start    │     └───────────────┘      │   │report    │
└────────┘   └──────────────────────────────────────────┘   └──────────┘
```

**Key configuration (loopConfig):**

- carried variable `report` (string, initially empty) with feedback selector = the reviewer Agent's `output` — each round writes that round's report back into `report`;
- end condition: reviewer output **contains** `"PASS"`;
- max rounds 3;
- output: `final_report` ← the reviewer's `output`;
- the reviewer prompt (excerpt) references a **cross-boundary variable** `{{#start.source#}}` (the snippet) and the **carried variable** `{{#loop-review.report#}}` (the previous report), and demands “output PASS alone on the last line if no defects remain”.

**Real run result** (verified on the installed app): all three review rounds executed and the Agent produced a report each round — but none contained PASS, so the Loop failed at the 3-round ceiling:

```text
Loop did not terminate within 3 rounds
```

This is exactly the ceiling semantics from [3.6](#36-loop-and-exit-loop): the end condition is evaluated after each round, a miss burns a round, and exhausting the ceiling fails. Three remedies:

1. **Raise the ceiling**: up to 100 rounds, leaving room for convergence;
2. **Make the convergence signal deterministic**: pin the report format in the prompt (e.g. “the report must start with DONE”) and switch the end condition from “contains” to “starts with” to reduce false matches;
3. **Switch to an Exit loop node**: put a Condition inside the body that connects to “Exit loop” when “remaining defects = 0”, ending the loop early.

The failure is **protective** by design: if exhausting the ceiling were treated as success, you would receive a not-yet-converged report without any warning.

### 5.3 Adapting the samples

- **Upgrade the iteration sample to object arrays**: change the Start variable to `array[object]` (JSON control) and read fields with `{{#iteration-1.item.title#}}`; keep the collect target.
- **Add conditional filtering**: put a Condition after the in-region Agent and bypass the collect target for filtered rounds (switch error strategy to `continue`, and accept that bypassed rounds count as failed — see [3.5](#35-iteration)).
- **Add automatic retry**: the reviewer is an ordinary Agent node — turn on “Retry on failure” and session-class failures rerun themselves.
- **Duplicate, don't destroy**: “Duplicate” in the row's `···` actions menu creates an editable, unpublished copy; the original stays runnable.

## 6. Scenario recipes

All recipes below come from the 22-scenario production suite (fully passing on latest main), each with a structure sketch and key configuration. Tests were driven through the real backend: create → publish → run → fill start inputs → poll to terminal state, with Agents executed by the fake plugin whose answer semantics match real Agents.

### 6.1 Conditional routing with an ELSE fallback

```text
Start ─▶ Agent (produce report) ─▶ Condition ─┬─ equals "critical" ─▶ Output A (alert)
                                             └─ ELSE ───────────────▶ Output B (record)
```

Points: string equality via “equals”; unmatched goes to the default branch; the inactive branch creates zero sessions and each Output binds its own branch's variables. When branch conditions deliberately overlap (two branches on `count > 3`), **the first branch wins** — either make them mutually exclusive or exploit first-match order on purpose.

### 6.2 Aggregating mutually exclusive branches

```text
        ┌─ Condition ─┬─ branch 1 ─ Agent-a ─┐
Start ──┤             └─ branch 2 ─ Agent-b ─┴─▶ Variable Aggregator ─▶ Output
```

Aggregator candidates `[agent-a.output, agent-b.output]`; only the executed side is assigned and the aggregator passes the first assigned one through. Remember `false`/empty strings count as “assigned”; if neither side ran, the node fails with `aggregator_no_match`.

### 6.3 Iteration with in-region filtering

```text
Start (array[object] PR list) ─▶ Iteration ─ [internal start ─▶ Agent ─▶ Condition ─┬─ needs_fix ─▶ Agent (fix; collect target)
                                                                                    └─ otherwise ──────────(bypasses the collect target)
```

Iterator source `start.prs` (array[object]); the in-region Condition compares `{{#iter.item.risk#}}`; error strategy `continue`. Rounds bypassing the collect target settle as failed rounds in `entries`, and `failed_count` is referenceable in an Output. If a bypass must not count as failure, guarantee every round passes the collect target (e.g. a lightweight Agent on the bypass branch too).

### 6.4 Loop feedback convergence

```text
Start (draft) ─▶ Loop [round start ─▶ writer Agent ─▶ reviewer Agent] ─▶ Output
```

Carried variable `draft` (string) with feedback = the reviewer's output; the writer prompt references `{{#loop.draft#}}` for draft iteration; the end condition is reviewer output `contains "LGTM"`; the output exports the final draft. Typed feedback works for object/number too — the carried variable's type drives in-body rendering.

### 6.5 Gated pipeline (Condition → Iteration → Loop → Output)

```text
Start ─▶ Condition (gate: mode equals "batch" AND threshold ≥ 1)
        ├─ match ─▶ Iteration (filter + collect) ─▶ Loop (feedback convergence) ─▶ Output
        └─ ELSE ──▶ Output (zero-session pass-through)
```

The three containers chain in sequence (Condition gate → Iteration → Loop, all at the top level; a Loop cannot nest inside an Iteration region). Verified scope conclusions: a Loop body can reference the Iteration's exposed `output` directly (it is an ancestor variable outside the loop boundary — the scenario suite asserts exactly this), and Start-declared object/array values flow across containers into loop bodies the same way. Inside the Iteration region, a Condition compares `{{#iter.item#}}` to filter elements, and filtered rounds bypass the collect target. When the gate misses, the ELSE branch's Output completes the run with zero sessions.

### 6.6 Structured output × iteration collection

An in-region Agent with structured output (schema like `{type:"object", properties:{fixed:{type:"boolean"}, note:{type:"string"}}}`), collect target `{member}.structured_output`, yields a typed `array[object]` collection that the Output node binds as a whole to hand back every result. A schema-invalid round fails with `structured_output` and triggers automatic retry, whose prompt carries the failure reason.

## 7. Best practices and pitfalls

**Variables and prompts**

- Only **explicitly referenced** upstream outputs enter a prompt; chain steps by writing `{{#agent-1.output#}}`, never rely on implicit injection.
- Referencing an unassigned variable = node failure (`prompt_template`). Cross-referencing parallel branches is a minefield: topological order does not guarantee the sibling finished first.
- Inside Loop bodies and for Loop outputs you will often hand-type references: the menu currently lists in-body products and globals only, so carried variables, outer variables, and named exports are written by hand (syntax in 2.3).
- To share configuration across the topology, use a dotted-name custom global (e.g. `cfg.threshold`) instead of edge detours.
- Condition chains are transparent: passing through a Condition does not change visible variables; but the Condition's own branch choice is not observable — if you need it, have a node inside the branch write a marker into its output.

**Start node**

- Always use Select for enumerations: out-of-option values are rejected at start with the variable named — far more robust than free text plus Condition checks.
- A JSON control's initial value must match the declared type; if you declared `array[object]`, do not store an object.
- The max-length constraint on string/secret governs both the editor default and run inputs.

**Condition and Aggregator**

- First match wins on overlap — either keep branches exclusive or order them to express priority.
- Unassigned variables go only with `empty`/`not_empty` (or engine-level `exists`/`not_exists`); other operators fail outright, which protects against stale-value misrouting.
- The aggregator provides no default; if you need a fallback, ensure every branch assigns — or use an ELSE branch so exactly one path always assigns.

**Iteration**

- Source length > max rounds = fails at start. For open-ended sources set the ceiling to a safe upper bound (it is a fuse, not a quota).
- Under `continue`, rounds bypassing the collect target count as failed — `failed_count` is the correct signal for “how many were filtered”.
- An empty array is legal input: zero rounds, immediate completion — handy for optional batch jobs.
- Iteration resume always restarts from round 1; never plan on resuming mid-round. Split expensive in-round work into separate runs.

**Loop**

- Design the convergence signal before the loop: the marker in the end condition must be produced **deterministically** by an in-body node (pin “output PASS alone on the last line” in the prompt), or you get the ceiling failure of the 5.2 sample.
- Ceiling failure is a fuse, not a bug: three non-converging rounds mean either the task does not converge or the signal is wrong.
- The carried variable's feedback selector decides “whose new value is taken at each round's end”; a broken chain (unassigned) fails loudly rather than reusing the old value.
- Loop bodies may contain Output and Exit loop nodes; no nested Loops/Iterations.
- A Loop with no end condition steered purely by Exit loop nodes is legal, but the round ceiling still applies.

**Agent and recovery**

- Interactive nodes are human-in-the-loop: right for approvals and clarification, wrong for unattended pipelines, and unavailable inside regions.
- Automatic retry covers only session-class and reply-class failures (six kinds, Appendix C); model-not-found and prompt-template errors fail at once and need a human.
- Retries do not roll back files — the Agent may have left half-applied changes; pair with “Resume + roll back to checkpoint” for file-sensitive work.
- Failures inside Iteration/Loop resume at the container with rounds from scratch; “roll back files” is unavailable for containers.

**Versions and runs**

- Runs freeze snapshots: editing the draft never affects a live run; publish a new version and start a new run to apply changes, or use snapshot-switch resume for a failed run (with compatibility checks).
- Confirm no active runs before deleting a workflow (it will be refused); versions frozen by runs cannot be deleted either.

## 8. Appendix: reference tables

### Appendix A: Value types

| Type                                                                   | Meaning                                                                                          |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `string` / `secret`                                                    | Strings; secret marks sensitive values semantically, renders like string, may carry a max length |
| `number` / `integer`                                                   | Number / integer; comparisons bridge integer and float                                           |
| `boolean`                                                              | Boolean                                                                                          |
| `file`                                                                 | Workspace file reference `{"kind":"workspace_file","path":"relative/path"}`                      |
| `array[file]`                                                          | Array of file references                                                                         |
| `object`                                                               | JSON object; selectors resolve nested paths                                                      |
| `any`                                                                  | Any JSON value                                                                                   |
| `array` / `array[any]`                                                 | Arrays with unconstrained elements (equivalent spellings; the latter states it explicitly)       |
| `array[string]` / `array[number]` / `array[object]` / `array[boolean]` | Typed arrays; every element validated                                                            |

### Appendix B: Comparison operators

| Operator                | Label (zh)     | Types          | Semantics                                                     |
| ----------------------- | -------------- | -------------- | ------------------------------------------------------------- |
| `equals`                | 等于           | any            | JSON equality; numbers bridge int/float                       |
| `not_equals`            | 不等于         | any            | Negation of the above                                         |
| `contains`              | 包含           | string / array | Substring for strings; element membership for arrays          |
| `not_contains`          | 不包含         | string / array | Negation of the above                                         |
| `starts_with`           | 开头是         | string         | Prefix match                                                  |
| `ends_with`             | 结尾是         | string         | Suffix match                                                  |
| `greater_than`          | 大于           | number         | Numeric comparison                                            |
| `greater_than_or_equal` | 大于等于       | number         | Numeric comparison                                            |
| `less_than`             | 小于           | number         | Numeric comparison                                            |
| `less_than_or_equal`    | 小于等于       | number         | Numeric comparison                                            |
| `empty`                 | 为空           | any            | Unassigned / null / empty string / empty array / empty object |
| `not_empty`             | 不为空         | any            | Negation of the above                                         |
| `is` / `is_not`         | (engine-level) | any            | Equality / negation, used for booleans                        |
| `exists` / `not_exists` | (engine-level) | any            | Whether the variable is assigned                              |

The UI offers the first 12; the `is`/`exists` families are engine-accepted for imported graphs and Loop end conditions. Type mismatches (e.g. “greater than” on a string) always fail rather than yield false.

### Appendix C: Failure kinds

`resumable` predicts “rerunning the same snapshot is a sensible first move” (environment/transient class) — a hint, not a gate; resume is always offered for a failed idle run. `Injected` marks failures injected into later attempts' prompts (run-level switch, default on).

| kind                                | Label (zh)                     | Auto-retry | resumable | Injected |
| ----------------------------------- | ------------------------------ | ---------- | --------- | -------- |
| `session`                           | 智能体会话失败                 | ✓          | ✓         | ✗        |
| `session_ended_without_stop_reason` | 会话异常结束                   | ✓          | ✓         | ✗        |
| `session_binding_rejected`          | 会话未能建立                   | ✓          | ✓         | ✗        |
| `structured_output`                 | 结构化输出不合格               | ✓          | ✗         | ✓        |
| `agent_refusal`                     | 智能体拒绝了请求               | ✓          | ✗         | ✓        |
| `unknown_stop_reason`               | 未知的停止原因                 | ✓          | ✗         | ✓        |
| `workflow_model_not_found`          | 模型不可用                     | ✗          | ✓         | ✗        |
| `missing_agent_config`              | 智能体配置缺失                 | ✗          | ✓         | ✗        |
| `baseline_persist`                  | 工作区基线保存失败             | ✗          | ✓         | ✗        |
| `repository`                        | 数据库操作失败                 | ✗          | ✓         | ✗        |
| `interrupted_by_restart`            | 被应用重启打断                 | ✗          | ✓         | ✗        |
| `missing_agent_ref`                 | 节点未指定智能体               | ✗          | ✗         | ✗        |
| `invalid_run_payload`               | 运行的冻结数据无效             | ✗          | ✗         | ✗        |
| `prompt_template`                   | 提示词模板无法渲染             | ✗          | ✗         | ✗        |
| `missing_skill_materialization`     | 技能未就绪                     | ✗          | ✗         | ✗        |
| `multiple_outputs`                  | 多个输出节点同时完成           | ✗          | ✗         | ✓        |
| `condition_evaluation`              | 条件无法判断                   | ✗          | ✗         | ✗        |
| `aggregator_no_match`               | 变量聚合器没有已产出的候选变量 | ✗          | ✗         | ✗        |

Note: the current scheduler resolves multiple completed Output nodes by precedence and finish time (see 3.7) and does not raise `multiple_outputs`; the kind is retained as a stable contract value.

### Appendix D: Terminology

| Chinese (UI) | English             | Graph `kind` |
| ------------ | ------------------- | ------------ |
| 开始         | Start               | `start`      |
| Agent        | Agent               | `agent`      |
| 条件分支     | Condition           | `condition`  |
| 变量聚合器   | Variable Aggregator | `aggregator` |
| 迭代         | Iteration           | `iteration`  |
| 循环         | Loop                | `loop`       |
| 退出循环     | Exit loop           | `loopExit`   |
| 输出         | Output              | `output`     |
| 舞台 / 全图  | Theater / Overview  | —            |
| 等待参与     | Awaiting input      | —            |
| 等待重试     | Waiting to retry    | —            |
| 从失败处继续 | Resume from failure | —            |

## Related documentation

- [Workflow architecture](workflow.md) / [中文](workflow.zh.md) — the full domain, engine, recovery, and snapshot semantics
- [Exit loop node](workflow-loop-exit.md) / [中文](workflow-loop-exit.zh.md) — exit execution and recovery details
- [Loop termination editor reference](dify-loop-termination-reference.md) / [中文](dify-loop-termination-reference.zh.md) — end-condition editor interaction reference
- [Session MCP](session-mcp.md) / [中文](session-mcp.zh.md) — runtime delivery of Agent-node MCP allowlists
- [Workflow execution scopes](workflow-execution-scopes.md) / [中文](workflow-execution-scopes.zh.md) — round scopes and persistence
