# Async Rust Learning Resources

Read these as their topics become relevant. Building an attempt before reading
a production implementation usually produces a better learning exercise.

## Foundations

- [The Rust Programming Language](https://doc.rust-lang.org/book/) - ownership,
  traits, trait objects, closures, threads, `Send`, and `Sync`.
- [The Rust Reference](https://doc.rust-lang.org/reference/) - precise language
  rules when the Book is not enough.
- [`std::future::Future`](https://doc.rust-lang.org/std/future/trait.Future.html) -
  the polling contract at the center of async Rust.
- [`std::task`](https://doc.rust-lang.org/std/task/) - `Context`, `Poll`, `Wake`,
  `Waker`, and `RawWaker`.
- [`std::pin`](https://doc.rust-lang.org/std/pin/) - pinning guarantees and
  projection concerns.
- [RFC 2349: Pin](https://rust-lang.github.io/rfcs/2349-pin.html) - the motivation
  and original design of `Pin` and `Unpin`.

## Futures, Executors, And Wakers

- [Asynchronous Programming in Rust](https://rust-lang.github.io/async-book/) -
  the Async Book; especially the execution chapters. It is undergoing a
  rewrite, so verify details against current standard-library documentation.
- [Tokio: Async in depth](https://tokio.rs/tokio/tutorial/async) - a compact
  end-to-end explanation of futures, executors, and wakeups.
- [Writing an OS in Rust: Async/Await](https://os.phil-opp.com/async-await/) -
  state machines, pinning, executors, and wakers in a constrained environment.
- [How Rust optimizes async/await, part 1](https://tmandry.gitlab.io/blog/posts/optimizing-await-1/) -
  how async functions become state machines.
- [Futures Explained in 200 Lines of Rust](https://cfsamson.github.io/books-futures-explained/) -
  a bottom-up model of futures and an event loop. Treat older API details as
  historical and retain the concepts.
- [Crust of Rust: Async/Await](https://www.youtube.com/watch?v=ThjvMReOXYM) -
  a detailed code-oriented lecture by Jon Gjengset.

## Async Design And Cancellation

- [Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/) - why
  cooperative executors must keep polling turns short.
- [The Async Rust `select!` macro](https://tokio.rs/tokio/tutorial/select) -
  multiplexing and the beginning of cancellation reasoning.
- [Async cancellation I](https://sunshowers.io/posts/cancelling-async-rust/) -
  cancellation through dropping futures and its consequences.
- [Barbara battles buffered streams](https://without.boats/blog/buffering/) -
  API design, buffering, and cancellation safety.
- [Keyword generics for async trait methods](https://smallcultfollowing.com/babysteps/blog/2022/09/18/dyn-async-traits-part-8-the-soul-of-rust/) -
  deeper context on async traits and the `Send` choices exposed by APIs.

## Timers And I/O

- [`std::thread::park`](https://doc.rust-lang.org/std/thread/fn.park.html) and
  [`Thread::unpark`](https://doc.rust-lang.org/std/thread/struct.Thread.html#method.unpark) -
  a simple blocking/wakeup mechanism and its token semantics.
- [Mio documentation](https://docs.rs/mio/latest/mio/) - portable, low-level
  non-blocking I/O and event notification.
- [Mio guide](https://docs.rs/mio/latest/mio/guide/) - registration, interests,
  tokens, and readiness handling.
- [epoll(7)](https://man7.org/linux/man-pages/man7/epoll.7.html),
  [kqueue(2)](https://man.freebsd.org/cgi/man.cgi?kqueue), and
  [I/O completion ports](https://learn.microsoft.com/en-us/windows/win32/fileio/i-o-completion-ports) -
  native event facilities underlying portable reactors. Read the one matching
  the platform being investigated.
- [The C10K problem](http://www.kegel.com/c10k.html) - historical context for
  scalable event-driven network servers.

## Concurrency And Scheduling

- [Rust Atomics and Locks](https://marabos.nl/atomics/) - atomics, memory
  ordering, channels, locks, and OS waiting primitives. Essential before a
  lock-free or multithreaded scheduler.
- [The Rustonomicon: Send and Sync](https://doc.rust-lang.org/nomicon/send-and-sync.html) -
  what thread-safety marker traits mean for tasks and runtime handles.
- [Crossbeam documentation](https://docs.rs/crossbeam/latest/crossbeam/) -
  concurrent queues, deques, epochs, and channels.
- [Crossbeam deque](https://docs.rs/crossbeam-deque/latest/crossbeam_deque/) -
  work-stealing queues when the multithreaded milestone is reached.
- [Loom](https://docs.rs/loom/latest/loom/) - exhaustive testing of small
  concurrent executions and memory-ordering assumptions.

## Runtime APIs And Real Applications

- [Tokio tutorial](https://tokio.rs/tokio/tutorial) - spawning, channels, I/O,
  framing, `select!`, and streams from a runtime user's perspective.
- [mini-redis](https://github.com/tokio-rs/mini-redis) - a deliberately small
  Tokio application showing realistic runtime patterns.
- [`futures` crate documentation](https://docs.rs/futures/latest/futures/) -
  common future, stream, task, channel, and I/O abstractions.
- [`AsyncRead`](https://docs.rs/futures/latest/futures/io/trait.AsyncRead.html) and
  [`AsyncWrite`](https://docs.rs/futures/latest/futures/io/trait.AsyncWrite.html) -
  poll-based asynchronous I/O trait design.
- [Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown) - cancellation
  signals, waiting for tasks, and runtime lifecycle.

## Production Implementations

Consult these after implementing the corresponding subsystem. Start from public
APIs and tests before optimized internals.

- [Tokio](https://github.com/tokio-rs/tokio) - multithreaded and current-thread
  schedulers, time, I/O, synchronization, and task machinery.
- [async-executor](https://github.com/smol-rs/async-executor) - a smaller
  production executor that is easier to survey than Tokio.
- [smol](https://github.com/smol-rs/smol) - a compact runtime assembled from
  focused crates.
- [Embassy executor](https://github.com/embassy-rs/embassy/tree/main/embassy-executor) -
  allocation-free async execution for embedded systems.
- [Glommio](https://github.com/DataDog/glommio) - a thread-per-core runtime built
  around Linux `io_uring`.

## Testing, Diagnostics, And Performance

- [Tokio: Unit testing](https://tokio.rs/tokio/topics/testing) - deterministic
  async tests and paused time.
- [Criterion.rs](https://bheisler.github.io/criterion.rs/book/) - statistical
  microbenchmarking.
- [tracing](https://docs.rs/tracing/latest/tracing/) - structured,
  async-aware diagnostics.
- [tokio-console](https://github.com/tokio-rs/console) - an example of runtime
  task instrumentation and diagnosing stalled tasks.
- [cargo-miri](https://github.com/rust-lang/miri) - undefined-behavior detection
  once low-level `unsafe` code appears.

## Optional Advanced Topics

- [`RawWaker`](https://doc.rust-lang.org/std/task/struct.RawWaker.html) and
  [`RawWakerVTable`](https://doc.rust-lang.org/std/task/struct.RawWakerVTable.html) -
  only after building a safe waker through `std::task::Wake`.
- [The Rustonomicon](https://doc.rust-lang.org/nomicon/) - unsafe Rust contracts.
- [io_uring](https://kernel.dk/io_uring.pdf) - completion-based Linux I/O and how
  it differs from readiness-based reactors.
- [Structured concurrency](https://vorpus.org/blog/notes-on-structured-concurrency-or-go-statement-considered-harmful/) -
  task ownership and lifetime design beyond detached spawning.
