//! Bound the checker's analyses: the recursive walkers `expr_source`, `analyze_expr_effect`,
//! `analyze_stmts` and the whole-struct lane's `walk` / `src` (`whole.rs`) in stack and closure
//! depth, and the whole request in memory (`crate::resource`).
//!
//! A closure can reach itself through the scope it is analyzed in (`g = |s| g(s)`, or `g` and `h`
//! calling each other after reassignment): each walker clones the scope, descends into the body,
//! finds a call of a name bound to a closure and descends into that closure's body again. The syntax
//! does not bound this, so the walk overflowed the stack. An acyclic chain of closures whose bodies
//! are deeply nested expressions overflowed it too (70 closures of 60 nested lists), and layered
//! closures that each call the previous one twice expand exponentially.
//!
//! Three limits, each fail-closed:
//! - less than 1 MiB of this thread's stack left, checked at every walker entry. This ends a cycle
//!   (whatever its shape) and any nesting too deep to analyze, and charges ordinary programs
//!   nothing for their expression depth. What it accepts depends on the thread's stack (the
//!   command line checks on a 64 MiB thread), as the crash it replaces did.
//! - more than 4096 nested descents that follow a name to a closure (a lambda written where it is
//!   analyzed nests only as deep as the source, which the stack guard bounds). A cycle ended only
//!   by the stack copies the scope at every descent; this ends it within 4096 of them.
//! - the request's memory budget (`crate::resource`, where the command line installs it): past
//!   it, the next guarded step refuses.
//!
//! Reaching one is sticky for the request: every later closure body or walker entry is cut short,
//! the walkers return a fail-closed answer, and `check` turns the request into an
//! `ANUBIS_ANALYSIS_LIMIT` refusal, so a partial analysis never passes as a check. The refusal is
//! reported alone: the other errors of such a request come from the fail-closed answers (a secret
//! "past the analysis limit" in a program that has none), so they are not findings.
//!
//! Time is not bounded. An analysis stopped by none of these can still take long (a chain of 8000
//! functions each calling the previous one runs past two minutes): it uses CPU, not the memory or
//! stack the rest of the machine depends on.
//!
//! The request-level refusal, its recovery between requests and the regression fixtures come from
//! the crash-diagnosis sessions of 2026-09-24 (`source_recursion.rs`); their guard counted every
//! expression, which refused valid programs such as a 40-term sum. Earlier versions here also
//! capped closure nesting at 64, 256 and 1024 (counting lambdas written in place), and the closure
//! bodies expanded under one outermost one at 16384, then 262144. Review showed each refused valid
//! programs the previous pin accepted (a 256-stage pipeline built by `compose`, closures layered
//! 10 deep in struct fields, five closures each nesting 205 lambdas), and that the whole-struct,
//! contract-discharge and contract-carrier lanes needed the stack guard too.
use std::cell::Cell;

const DEPTH_LIMIT: usize = 4096;
/// Stack kept free below the deepest walker entry: covers the helpers that recurse over one
/// expression (at most the parser's nesting bound) between two checks.
const RED_ZONE: usize = 1 << 20;
/// Stack a request may use when the platform does not report the thread's stack.
const FALLBACK_BUDGET: usize = 4 << 20;
pub(super) const DIAGNOSTIC: &str = "ANUBIS_ANALYSIS_LIMIT: the checker ran out of stack or of \
     its memory budget, or nested closure bodies more than 4096 deep, while analyzing this program \
     (for example a closure that calls itself through a reassigned name, directly or through \
     another closure, or expressions nested too deeply inside a chain of calls); it cannot bound \
     what the program returns or does, so the program is refused. This is a limit of the checker, \
     not a finding about the program: simplify the program, or give the check more memory \
     (ANUBIS_ANALYSIS_MEMORY_MIB, in MiB) if the machine has it to spare";

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static EXHAUSTED: Cell<bool> = const { Cell::new(false) };
    /// The lowest stack address a walker may enter at, in the current request (0: no request).
    static FLOOR: Cell<usize> = const { Cell::new(0) };
}

/// An address in the current stack frame.
#[inline(always)]
fn stack_pointer() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// The lowest address of this thread's stack, where the platform reports it.
#[cfg(target_os = "linux")]
fn stack_bottom() -> Option<usize> {
    // SAFETY: `attr` is initialized by `pthread_getattr_np` before it is read, and destroyed once.
    unsafe {
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
            return None;
        }
        let mut addr: *mut libc::c_void = std::ptr::null_mut();
        let mut size: libc::size_t = 0;
        let ok = libc::pthread_attr_getstack(&attr, &mut addr, &mut size) == 0;
        libc::pthread_attr_destroy(&mut attr);
        (ok && !addr.is_null()).then_some(addr as usize)
    }
}

#[cfg(target_os = "macos")]
fn stack_bottom() -> Option<usize> {
    // SAFETY: both calls only read the current thread's own attributes.
    unsafe {
        let this = libc::pthread_self();
        let top = libc::pthread_get_stackaddr_np(this) as usize;
        top.checked_sub(libc::pthread_get_stacksize_np(this))
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn stack_bottom() -> Option<usize> {
    None
}

/// Whether the walker about to run must be cut short: a limit was reached earlier in the request,
/// or the stack or the memory budget is nearly used up (which is then recorded).
pub(super) fn cut() -> bool {
    if EXHAUSTED.get() {
        return true;
    }
    if stack_pointer() < FLOOR.get() || crate::resource::over_budget() {
        EXHAUSTED.set(true);
        return true;
    }
    false
}

/// One closure-body descent in progress.
pub(super) struct Frame;

impl Frame {
    /// Enter a closure body, or `None` once a limit is reached (and the request is then refused).
    pub(super) fn enter() -> Option<Self> {
        let depth = DEPTH.get();
        if cut() || depth >= DEPTH_LIMIT {
            EXHAUSTED.set(true);
            return None;
        }
        DEPTH.set(depth + 1);
        Some(Self)
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        DEPTH.set(DEPTH.get() - 1);
    }
}

/// Run one check request. A limit reached anywhere inside it refuses the request: the limit comes
/// first, then any other errors (found by an analysis that was cut short). Each request starts
/// clean (an LSP or batch run checks many files in one process); the previous state is restored on
/// return and on unwind, so requests may nest.
pub(super) fn check<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    struct Request(bool, usize, Option<(usize, usize, bool)>);
    impl Drop for Request {
        fn drop(&mut self) {
            EXHAUSTED.set(self.0);
            FLOOR.set(self.1);
            if let Some(prev) = self.2.take() {
                crate::resource::disarm(prev);
            }
        }
    }
    let here = stack_pointer();
    let floor = match stack_bottom() {
        Some(bottom) if bottom < here => bottom.saturating_add(RED_ZONE),
        _ => here.saturating_sub(FALLBACK_BUDGET),
    };
    let _request = Request(
        EXHAUSTED.replace(false),
        FLOOR.replace(floor.max(FLOOR.get())),
        crate::resource::arm(),
    );
    let result = f();
    if !EXHAUSTED.get() {
        return result;
    }
    Err(DIAGNOSTIC.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Closure descents that never end (the depth cap stops them).
    #[inline(never)]
    fn recurse() {
        if let Some(_frame) = Frame::enter() {
            let pad = std::hint::black_box([0u8; 512]);
            recurse();
            std::hint::black_box(pad);
        }
    }

    /// Recursion with a real stack frame, cut only by the stack guard.
    #[inline(never)]
    fn deep(n: u64) -> u64 {
        if cut() {
            return 0;
        }
        let pad = std::hint::black_box([n; 64]);
        pad[(n % 64) as usize] + deep(n + 1)
    }

    #[test]
    fn endless_descent_refuses_and_next_request_recovers() {
        let err = check(|| {
            recurse();
            Ok(())
        })
        .unwrap_err();
        assert!(err.starts_with("ANUBIS_ANALYSIS_LIMIT"), "{err}");
        assert_eq!(DEPTH.get(), 0);
        assert_eq!(
            check(|| {
                let _frame = Frame::enter().unwrap();
                Ok(())
            }),
            Ok(())
        );
    }

    #[test]
    fn breadth_is_not_limited() {
        assert_eq!(
            check(|| {
                let _root = Frame::enter().unwrap();
                for _ in 0..1_000_000 {
                    let _child = Frame::enter().unwrap();
                }
                Ok(())
            }),
            Ok(())
        );
    }

    #[test]
    fn closure_depth_is_limited_at_4096() {
        fn nest(n: usize) -> bool {
            match Frame::enter() {
                Some(_frame) => n == 0 || nest(n - 1),
                None => false,
            }
        }
        // A thread with room for the nesting, so only the depth limit can stop it.
        let (within, past) = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(|| {
                let within = check(|| Ok(nest(DEPTH_LIMIT - 1)));
                let past = check(|| Ok(nest(DEPTH_LIMIT)));
                (within, past)
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(within, Ok(true));
        assert!(past.unwrap_err().starts_with("ANUBIS_ANALYSIS_LIMIT"));
    }

    #[test]
    fn a_reached_limit_cuts_everything_after_it() {
        let err = check(|| {
            recurse();
            assert!(cut());
            assert!(Frame::enter().is_none());
            Ok(())
        })
        .unwrap_err();
        assert!(err.starts_with("ANUBIS_ANALYSIS_LIMIT"), "{err}");
    }

    #[test]
    fn stack_exhaustion_refuses_instead_of_overflowing() {
        // A 2 MiB thread: the guard must stop the recursion with the red zone still free.
        let err = std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(|| {
                check(|| {
                    deep(0);
                    Ok(())
                })
            })
            .unwrap()
            .join()
            .unwrap()
            .unwrap_err();
        assert!(err.starts_with("ANUBIS_ANALYSIS_LIMIT"), "{err}");
    }

    /// Past the limit the other errors come from fail-closed answers (review of the checker limits:
    /// a program with no secret was refused with a secret "past the analysis limit" beside the
    /// limit), so the refusal is the limit alone. It is still a refusal.
    #[test]
    fn the_limit_is_reported_alone() {
        let err = check::<()>(|| {
            recurse();
            Err("ANUBIS_SECRET_EXFILTRATION: x".into())
        })
        .unwrap_err();
        assert_eq!(err, DIAGNOSTIC);
    }

    #[test]
    fn nested_request_keeps_the_outer_refusal() {
        let err = check(|| {
            recurse();
            check(|| Ok(()))
        })
        .unwrap_err();
        assert!(err.starts_with("ANUBIS_ANALYSIS_LIMIT"), "{err}");
    }

    #[test]
    fn unwind_restores_request_and_depth() {
        let _ = std::panic::catch_unwind(|| {
            check::<()>(|| {
                let _frame = Frame::enter().unwrap();
                recurse();
                panic!("test unwind");
            })
        });
        assert_eq!(DEPTH.get(), 0);
        assert_eq!(FLOOR.get(), 0);
        assert_eq!(check(|| Ok(())), Ok(()));
    }
}
