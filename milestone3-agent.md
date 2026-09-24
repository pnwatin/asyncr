**Milestone 3 Scope**
Milestone 3 turns your executor from “a queue of unit-returning futures” into a small task runtime with:

- Runtime-owned task lifecycles
- Dynamic spawning while the executor is running
- Values returned from tasks
- Explicit ownership and shutdown semantics

It does **not** require timers, I/O, cancellation APIs, multithreading, or performance work.

**Current Architecture**
Your current `Task` combines two responsibilities:

- Thread-safe scheduling state used by the waker
- Ownership and polling of the user’s future

This appears in `src/task.rs:9-12`, with the future stored inside `TaskState`.

That coupling creates several current limitations:

- `Wake` requires thread-safe scheduling state.
- Because the future is inside that state, every future must be `Send`.
- `spawn` only accepts `Output = ()`.
- The runtime cannot return task results.
- There is no distinction between task lifetime and stale waker lifetime.
- Spawning is only available through `&Runtime`, but `run` consumes the runtime.

Milestone 3 addresses these connections.

**Goals Against Current Code**

| Goal | Current state | Milestone target |
| --- | --- | --- |
| Separate task harness from its future | Combined inside `Task` and `TaskState` | Scheduling metadata can exist independently from the user future being polled |
| Spawn through a clonable handle | Only `Runtime::spawn`; runtime is consumed by `run` | A handle can be cloned and used by running tasks or other owners |
| `block_on` returning a value | `run` returns `()` | Drive one root future and return its output |
| `JoinHandle` future | No result channel or result-waker state | Spawning returns something awaitable that produces the task result |
| Define dropped join handles | Undefined | Explicitly choose detach or cancellation |
| Define panic behavior | A panic unwinds `run` and may poison task state | Decide how the joining code observes task failure |
| Support `!Send` tasks | `Send` required at `runtime.rs:22` and `task.rs:95` | Demonstrate a valid task containing `Rc`, `RefCell`, or another `!Send` value |
| Test lifecycle and shutdown | Only queue/wake tests exist | Cover spawn, join, detachment, and when execution terminates |

**The Central Design Problem**
The most important challenge is separating:

1. **The future:** mutable state polled only on the executor thread.
2. **The task:** runtime ownership, completion, result, and cancellation state.
3. **The wake handle:** thread-safe capability that marks or queues the task.
4. **The join handle:** observer waiting for the task’s result.
5. **The spawn handle:** capability to introduce new tasks.
6. **The runtime:** owner of scheduling and shutdown.

Your current `Arc<Task>` represents almost all of those at once. Milestone 3 asks you to pull those roles apart enough that their lifetimes become understandable.

This separation is also likely how you remove the `Send` requirement: the thread-safe waker should not have to own the `!Send` future directly. The future can remain exclusively owned and polled by the executor thread while the waker carries only thread-safe scheduling information.

**Recommended Semantics**
For this learning milestone, the simplest choices are:

- Dropping a `JoinHandle` **detaches** the task.
- A detached task continues running, but its output is discarded.
- Explicit cancellation remains deferred until Milestone 9.
- `block_on` returns when its root future completes.
- Decide separately whether detached tasks continue after the root completes.
- A panic should have defined observable behavior rather than accidentally poisoning internal state.

Detachment is preferable here because making `JoinHandle::drop` cancel the task introduces cancellation invariants far ahead of the cancellation milestone.

**Suggested Implementation Order**

1. Write down lifecycle and shutdown rules.
2. Separate executor-local future storage from wake/scheduling state.
3. Introduce a clonable spawn handle.
4. Allow spawned futures to return generic outputs.
5. Create result state shared between the task and `JoinHandle`.
6. Make `JoinHandle` implement `Future`.
7. Build `block_on` using the task/result machinery.
8. Remove `Send` from executor-local futures.
9. Add lifecycle and shutdown tests.

Avoid starting with `JoinHandle`. The current coupling between `Wake` and `TaskFuture` will otherwise force `Send` through the new APIs and make the design harder to untangle later.

**Definition Of Done**
Useful deterministic tests should establish:

- A running task can spawn a child.
- Joining a pending child suspends the parent rather than busy-polling it.
- Joining an already-completed child returns immediately.
- `block_on` returns a non-`()` value.
- Dropping a join handle follows the chosen detach policy.
- A detached task behaves according to the documented shutdown rule.
- A task using `Rc` or `RefCell` compiles and runs.
- A task panic has the chosen observable result.
- Shutdown is not accidentally controlled by stale wakers.
- The executor exits under your documented shutdown conditions.

Before changing the architecture, make these three decisions explicitly:

1. Does `block_on` stop immediately when the root completes, or finish all spawned tasks?
2. Does dropping `JoinHandle` detach? I recommend yes.
3. Does a task panic become a join error, or terminate the entire runtime invocation?
