# AGENTS.md

C++ and cxx-bridge code in this crate. Follow the repository-root `AGENTS.md` for Rust-wide rules; this file covers native ownership and teardown.

## Memory lifecycle

C++ `shared_ptr` graphs and WebRTC observer registrations can leak independently of Rust `Drop`.

- Treat every `shared_ptr` copy, callback capture, observer registration, and spawned thread as an ownership edge. If an owner transitively stores the callback, the callback must not strongly capture that owner.
- Use `weak_ptr` for non-owning back-references. A strong back-reference is allowed only when teardown explicitly unregisters or clears it before releasing the owner, and a lifecycle regression test proves cleanup.
- A callback stored by an object must not capture a strong clone of that same object. Reviewers must flag C++ lambdas capturing `shared_ptr`/`shared_from_this` when the receiver can retain the callback.
- Every thread, native handle, observer, timer, and queue must have a named owner and deterministic shutdown path. Threads must be joined; observers must be unregistered; native resources (CUDA contexts, FDs, WebRTC factories) must be released on the teardown path.
- For lifecycle-sensitive changes, test observable destruction: `weak_ptr::expired()`, native handle maps empty, and thread/FD counts returning near baseline. RAII destructors alone are not evidence that destruction occurs because reference cycles prevent them from running.
- Any new C++-reachable feature must add a repeated `initialize → use → disconnect/drop → shutdown` workload. Include success, failure/cancellation, and partial-initialization teardown where applicable.
- Review ownership changes by sketching the strong-reference graph. Every cycle must contain a weak edge or an explicit, tested unlink operation.

When reviewing changed code, search for `shared_from_this`, `shared_ptr` copies into lambdas, listener/observer registration, and native handle stores. Report lifecycle risks even when the code compiles.
