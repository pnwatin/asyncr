# asyncr

`asyncr` is a learning project: build an async Rust runtime one concept at a
time, understand each limitation, and replace it only when the next idea calls
for it. It is not intended for production use or immediate Tokio compatibility.

The checklist is ordered. Each milestone should remain small enough to explain
without referring to its code, and should have deterministic tests before it is
considered complete. Relevant reading is collected in [RESSOURCES.md](RESSOURCES.md).

## 0. Language Foundations

Concept: the language supplies state machines and polling interfaces, while a
runtime supplies execution.

- [x] Understand that calling an async function creates a lazy future.
- [x] Study `Future`, `Poll`, `Context`, `Waker`, `Pin`, and `Unpin`.
- [x] Understand why compiler-generated futures may be self-referential.
- [x] Distinguish concurrency, parallelism, and asynchronous I/O.
- [x] Distinguish an executor, scheduler, reactor, and runtime.

## 1. Manual Futures And Naive Execution

Concept: polling advances a state machine, and cooperative tasks only make
progress when polled.

- [x] Implement a future that yields once.
- [x] Implement a future that completes after several polls.
- [x] Store heterogeneous, pinned futures as tasks.
- [x] Implement a single-threaded FIFO executor.
- [x] Requeue every pending task and deliberately busy-poll it.
- [x] Run manual futures and compiler-generated async blocks together.
- [x] Define and test countdown behavior for zero and one.
- [x] Test poll order, task completion, and nested awaits without relying on
  console output.

Completion check: explain exactly why this executor consumes CPU while no task
can make progress and why a completed future must never be polled again.

## 2. The Wakeup Contract

Concept: `Pending` means "poll me after I signal readiness," not "poll me
continuously."

- [x] Make a task capable of scheduling itself through a `Waker`.
- [x] Build a safe waker with `std::task::Wake` before studying `RawWaker`.
- [x] Replace unconditional requeueing with a ready queue.
- [x] Let the executor block while the ready queue is empty.
- [x] Handle wake-before-sleep without losing the notification.
- [x] Handle wakeups during polling and harmless spurious wakeups.
- [x] Prevent duplicate queue entries without losing a real wakeup.
- [x] Test that a pending task is not polled until it wakes.

Completion check: trace one task from `poll` to `Pending`, through `wake`, back
into the queue, and into its next `poll`.

## 3. Tasks, Spawning, And Results

Concept: a task is a future plus runtime-owned lifecycle and scheduling state.

- [x] Separate the task harness from the future it owns.
- [x] Allow a running task to spawn another task through a clonable handle.
- [x] Add `block_on` for one root future that returns a value.
- [x] Return task results through a `JoinHandle` future.
- [x] Define whether dropping a `JoinHandle` detaches or cancels its task.
- [x] Decide how task panics are represented and observed.
- [x] Remove unnecessary `Send` bounds from the single-threaded runtime and
  demonstrate a valid `!Send` task.
- [x] Test spawning, joining, detachment, and executor shutdown conditions.

Completion check: distinguish the lifetime of a future, its task, its join
handle, and the runtime that owns scheduling.

## 4. A First Real Asynchronous Event

Concept: an external producer stores a waker and signals it when a resource
becomes ready.

- [ ] Implement a delay future using one helper thread as the event source.
- [ ] Store and replace the most recently supplied waker correctly.
- [ ] Ensure repeated polls do not create repeated helper threads.
- [ ] Handle a delay that is already elapsed.
- [ ] Explore what happens when the delay future is dropped before firing.
- [ ] Test that the executor sleeps rather than spins while waiting.

Completion check: explain why a future may receive a different waker on every
poll and when `Waker::will_wake` is useful.

## 5. Timer Driver

Concept: many logical timers should share one physical waiting mechanism.

- [ ] Replace one-thread-per-delay with a runtime timer driver.
- [ ] Order deadlines with a suitable data structure.
- [ ] Sleep until the next deadline or an earlier timer is inserted.
- [ ] Give timer registrations stable identities.
- [ ] Remove or ignore cancelled timers safely.
- [ ] Define behavior for equal deadlines and very long durations.
- [ ] Add timeout and interval futures.
- [ ] Test timers with a controllable clock rather than wall-clock sleeps.

Completion check: explain how the scheduler and timer driver avoid both busy
polling and oversleeping a newly inserted earlier deadline.

## 6. Non-Blocking I/O And The Reactor

Concept: the reactor maps operating-system readiness events to task wakeups.

- [ ] Learn non-blocking sockets and the meaning of `WouldBlock`.
- [ ] Use `mio` to register a TCP listener with the operating system.
- [ ] Associate registrations with stable tokens.
- [ ] Wake the task interested in each readiness event.
- [ ] Integrate reactor waiting with scheduler wakeups and timer deadlines.
- [ ] Handle deregistration and stale events without waking the wrong task.
- [ ] Understand level-triggered and edge-triggered readiness.
- [ ] Test more connections than there are runtime threads.

Completion check: trace a socket read from `WouldBlock`, through OS readiness,
to the task's successful next poll.

## 7. Async TCP Types

Concept: readiness is only a hint; non-blocking operations must still be
retried and partial progress is normal.

- [ ] Wrap non-blocking TCP accept, connect, read, and write operations in
  futures or poll-based methods.
- [ ] Correctly handle partial reads and writes.
- [ ] Re-register interest after `WouldBlock` as required by the reactor model.
- [ ] Support independent read and write waiters or explicitly document the
  chosen limitation.
- [ ] Define minimal `AsyncRead` and `AsyncWrite`-like traits, then compare them
  with ecosystem traits.
- [ ] Build an echo server using only this runtime.
- [ ] Test disconnects, half-closes, backpressure, and large payloads.

Completion check: explain why "writable" does not mean an entire buffer can be
written and why readiness races are expected rather than exceptional.

## 8. Future Composition

Concept: async control-flow utilities are themselves futures with state and
cancellation behavior.

- [ ] Implement `join` for concurrent completion.
- [ ] Implement `select` or a race combinator.
- [ ] Implement timeout by racing work against a timer.
- [ ] Implement a reusable yield-now future using the real scheduler.
- [ ] Explore fused futures and polling after completion.
- [ ] Define cancellation safety for each combinator.
- [ ] Test biased selection, fairness, and simultaneous readiness.

Completion check: identify exactly which child future is dropped on every
branch and whether dropping it can lose data or violate an invariant.

## 9. Cancellation And Task Lifecycle

Concept: Rust cancellation normally occurs by dropping an in-progress future.

- [ ] Add explicit task abort support.
- [ ] Decide when destructors run after cancellation.
- [ ] Ensure cancelled tasks release reactor and timer registrations.
- [ ] Explore cancellation-safe and cancellation-unsafe operations.
- [ ] Add cooperative runtime shutdown.
- [ ] Distinguish graceful shutdown, immediate shutdown, and detached tasks.
- [ ] Test cancellation at every await point of a small stateful operation.

Completion check: explain what state remains externally visible if each async
operation is dropped at any suspension point.

## 10. Async Synchronization

Concept: async synchronization suspends tasks rather than blocking executor
threads.

- [ ] Build a one-shot channel and use its receiver as a future.
- [ ] Build a notification primitive and reason about stored permits.
- [ ] Build a bounded multi-producer, single-consumer channel.
- [ ] Implement backpressure when the channel is full.
- [ ] Build an async mutex after understanding its waiter queue.
- [ ] Explore fairness, cancellation of waiters, and wakeup ownership.
- [ ] Compare when `std::sync::Mutex` is preferable to an async mutex.

Completion check: demonstrate that no executor thread blocks while a task waits
and that cancelling a waiter does not strand a notification or lock.

## 11. Streams, Framing, And Backpressure

Concept: asynchronous sequences extend one-result futures with repeated
readiness and termination.

- [ ] Implement a minimal `Stream`-like trait.
- [ ] Turn incoming connections or channel messages into a stream.
- [ ] Parse framed messages across arbitrary read boundaries.
- [ ] Buffer writes while enforcing a memory limit.
- [ ] Propagate backpressure through multiple layers.
- [ ] Build a small request-response server with multiple clients.
- [ ] Test malformed frames, slow peers, and bounded memory use.

Completion check: explain where buffering occurs and what prevents a slow peer
from causing unbounded memory growth.

## 12. Fairness And Cooperative Scheduling

Concept: a ready task can monopolize a cooperative executor unless the runtime
and its resources impose a budget.

- [ ] Demonstrate starvation with a future that never yields.
- [ ] Add a per-task cooperative polling or operation budget.
- [ ] Choose FIFO or another ready-queue policy and document its tradeoffs.
- [ ] Explore priority inversion and deliberately avoid premature priorities.
- [ ] Measure wake-to-poll latency under load.
- [ ] Test that always-ready I/O cannot indefinitely starve timers or peers.

Completion check: state the runtime's fairness guarantee and construct an
adversarial workload that tests it.

## 13. Multithreaded Execution

Concept: parallel polling introduces synchronization, task migration, and
stronger `Send`/`Sync` requirements.

- [ ] Start with several workers sharing one synchronized ready queue.
- [ ] Ensure one future is never polled concurrently by two workers.
- [ ] Make wakeups safe from any thread.
- [ ] Define which spawned futures and outputs must be `Send`.
- [ ] Keep or design a path for local `!Send` tasks.
- [ ] Handle worker sleep and notification without missed wakeups.
- [ ] Test wake/poll races with Loom where practical.

Completion check: identify the synchronization protecting every mutable task
state and justify each `Send` and `Sync` bound in the public API.

## 14. Work Stealing

Concept: per-worker queues improve locality while stealing balances uneven
workloads.

- [ ] Give each worker a local queue plus a global injection queue.
- [ ] Let idle workers steal tasks from busy workers.
- [ ] Decide where externally woken and newly spawned tasks are queued.
- [ ] Explore local batching and cache locality.
- [ ] Prevent sleepers from missing newly available work.
- [ ] Measure balanced, imbalanced, and wake-heavy workloads.
- [ ] Validate concurrent queue assumptions with Loom or an established queue.

Completion check: explain the path of a locally spawned task, an externally
woken task, and a stolen task through the scheduler.

## 15. Blocking Work

Concept: unavoidable blocking operations need isolation from async workers.

- [ ] Add a separate blocking thread pool.
- [ ] Return blocking-operation results through awaitable handles.
- [ ] Bound thread creation and queued work.
- [ ] Define shutdown behavior for running blocking operations.
- [ ] Explore why aborting a future cannot generally interrupt a blocking
  system call or arbitrary function.
- [ ] Test runtime responsiveness while blocking jobs run.

Completion check: demonstrate that saturating the blocking pool does not stop
timers and network tasks from progressing.

## 16. Runtime Context And Ergonomics

Concept: convenient APIs need an explicit answer to "which runtime owns this
resource?"

- [ ] Introduce runtime and spawn handles deliberately.
- [ ] Decide whether APIs require an entered runtime context.
- [ ] Detect accidental nested runtimes or document their behavior.
- [ ] Add a builder for thread count and subsystem configuration.
- [ ] Explore current-thread versus multithreaded runtime flavors.
- [ ] Consider macros only after the underlying APIs are stable and understood.

Completion check: explain how any timer, I/O handle, or spawned task finds the
runtime subsystem it depends on.

## 17. Observability And Debugging

Concept: invisible task state makes async stalls difficult to diagnose.

- [ ] Assign task IDs and human-readable task names.
- [ ] Instrument spawn, poll, wake, completion, and cancellation events.
- [ ] Record queue depth, active tasks, poll duration, and wake latency.
- [ ] Detect tasks whose polls block executor threads for too long.
- [ ] Provide a snapshot of tasks and the resources they await.
- [ ] Use structured tracing rather than relying on `println!`.

Completion check: diagnose a deliberately stalled task from instrumentation
without attaching a debugger or adding new log statements.

## 18. Deterministic Testing And Verification

Concept: concurrency needs control over time, scheduling, and race exploration.

- [ ] Add a paused, manually advanced clock.
- [ ] Make scheduler tests deterministic where possible.
- [ ] Property-test task state machines and registration lifecycles.
- [ ] Use Loom for focused synchronization models.
- [ ] Use Miri when low-level unsafe code is introduced.
- [ ] Add stress tests for wake storms, cancellation, and shutdown.
- [ ] Verify that tests fail when known lost-wakeup bugs are injected.

Completion check: reproduce timing-sensitive failures without depending on
arbitrary sleeps or repeated luck.

## 19. Performance And Memory

Concept: optimize measured bottlenecks while preserving scheduler correctness.

- [ ] Establish benchmarks for spawn, wake, scheduling, timers, and I/O.
- [ ] Measure throughput and tail latency, not only averages.
- [ ] Count allocations per task and per wakeup.
- [ ] Explore intrusive task queues only after profiling justifies them.
- [ ] Explore task-state bit packing and atomics only with documented
  invariants.
- [ ] Compare behavior against established runtimes without treating benchmark
  victory as the project's goal.
- [ ] Re-run correctness and concurrency tests after every optimization.

Completion check: every optimization names the measured bottleneck, preserves
an explicit invariant, and includes before-and-after evidence.

## 20. Production Questions

Concept: a useful learning runtime and a dependable production runtime have
very different completion bars.

- [ ] Audit all unsafe code and document safety invariants.
- [ ] Define supported platforms and I/O backends.
- [ ] Handle resource exhaustion and overload intentionally.
- [ ] Review panic containment and failure isolation.
- [ ] Review API compatibility and semantic guarantees.
- [ ] Fuzz protocol and I/O boundary code.
- [ ] Study Tokio, smol, async-executor, Embassy, and Glommio to compare design
  choices with the ones reached independently here.

Completion check: write down what remains intentionally unsupported and why
this project should still not be used in production.
