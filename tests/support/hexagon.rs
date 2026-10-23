//! Stack allocation for the scalar Hexagon lifter in debug test builds.

pub fn run_on_large_lifter_stack<F, R>(name: &str, f: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    // The scalar Hexagon opcode lifter has a large debug stack frame. Keep
    // lifter-heavy tests independent of the harness thread stack size.
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name(name.into())
            .stack_size(64 * 1024 * 1024)
            .spawn_scoped(scope, f)
            .expect("failed to spawn large-stack Hexagon lifter test thread");
        match handle.join() {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}
