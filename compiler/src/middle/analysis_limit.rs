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
//! `ANUBIS_ANALYSIS_LIMIT` refusal, so a partial analysis never passes as a check. The refusal names
//! the limit that was reached (only the memory budget can be raised), and is followed, on the next
//! line, by the findings the analysis made BEFORE it reached the limit: those came from a complete
//! analysis of what it had covered. Every diagnostic raised after it is dropped, by position
//! (`SemanticContext::push_diag` marks where the limit was first seen), never by its text: those
//! came from the fail-closed answers (a secret "past the analysis limit", or a public binding
//! "initialized from a secret value", in a program with none). What the analysis did not reach
//! was not analyzed at all, so a finding there is not reported; the program is refused regardless.
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
/// No limit reached; the stack guard; the closure-depth cap; the memory budget.
const NONE: u8 = 0;
const STACK: u8 = 1;
const DEPTH: u8 = 2;
const MEMORY: u8 = 3;

/// The limit a check request reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisLimit {
    /// The stack guard: nesting too deep to analyze (deterministic for a given thread's stack).
    Stack,
    /// The closure-depth cap (deterministic).
    Depth,
    /// The memory budget (depends on the memory the machine had free).
    Memory,
}

/// How the memory refusal begins (the JSON lane reads its budget from this text, which is ours).
pub(crate) const MEMORY_PREFIX: &str =
    "ANUBIS_ANALYSIS_LIMIT: the checker's analysis of this program exceeded its memory budget (";

/// The limit the last check request on this thread reached, if any.
pub(crate) fn last() -> Option<AnalysisLimit> {
    match LAST.get() {
        STACK => Some(AnalysisLimit::Stack),
        DEPTH => Some(AnalysisLimit::Depth),
        MEMORY => Some(AnalysisLimit::Memory),
        _ => None,
    }
}

/// The refusal for the limit that was reached.
pub(super) fn diagnostic(reason: u8) -> String {
    let refused = "it cannot bound what the program returns or does past that point, so the \
                   program is refused (any findings made before it are listed after this)";
    match reason {
        MEMORY => format!(
            "{MEMORY_PREFIX}{} MiB: half the memory free when the check began, or \
             ANUBIS_ANALYSIS_MEMORY_MIB); {refused}. This is a limit of the checker, not a finding \
             about the program: simplify it, or give the check more memory \
             (ANUBIS_ANALYSIS_MEMORY_MIB, in MiB) if the machine has it to spare",
            crate::resource::soft_budget_mib()
        ),
        DEPTH => format!(
            "ANUBIS_ANALYSIS_LIMIT: the checker followed more than {DEPTH_LIMIT} nested closure \
             bodies while analyzing this program; {refused}. This is a limit of the checker: a \
             closure that calls itself through a reassigned name (directly or through another \
             closure) reaches it, and so do closures nested that deep; reduce the nesting, or make \
             such a closure a named function. More memory does not change it"
        ),
        _ => format!(
            "ANUBIS_ANALYSIS_LIMIT: the checker ran out of stack while analyzing this program \
             (expressions or closure bodies nested too deeply to analyze, or a closure that reaches \
             itself through a reassigned name); {refused}. This is a limit of the checker, not a \
             finding about the program: reduce the nesting. More memory does not change it"
        ),
    }
}

thread_local! {
    static DEPTH_NOW: Cell<usize> = const { Cell::new(0) };
    /// The limit reached in the current request (`NONE` while none is).
    static REASON: Cell<u8> = const { Cell::new(NONE) };
    /// The limit the last finished request reached (`last`).
    static LAST: Cell<u8> = const { Cell::new(NONE) };
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
    if REASON.get() != NONE {
        return true;
    }
    if stack_pointer() < FLOOR.get() {
        REASON.set(STACK);
        return true;
    }
    if crate::resource::over_budget() {
        REASON.set(MEMORY);
        return true;
    }
    false
}

/// Whether a limit was reached in the current request.
pub(super) fn exhausted() -> bool {
    REASON.get() != NONE
}

/// One closure-body descent in progress.
pub(super) struct Frame;

impl Frame {
    /// Enter a closure body, or `None` once a limit is reached (and the request is then refused).
    pub(super) fn enter() -> Option<Self> {
        let depth = DEPTH_NOW.get();
        if cut() {
            return None;
        }
        if depth >= DEPTH_LIMIT {
            REASON.set(DEPTH);
            return None;
        }
        DEPTH_NOW.set(depth + 1);
        Some(Self)
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        DEPTH_NOW.set(DEPTH_NOW.get() - 1);
    }
}

/// Run one check request. A limit reached anywhere inside it refuses the request: the limit comes
/// first, then, on the next line, the other errors (the genuine findings; see `typecheck_request`).
/// Each request starts clean (an LSP or batch run checks many files in one process); the previous
/// state is restored on return and on unwind, so requests may nest.
pub(super) fn check<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    struct Request(u8, usize, Option<(usize, usize, bool)>);
    impl Drop for Request {
        fn drop(&mut self) {
            REASON.set(self.0);
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
        REASON.replace(NONE),
        FLOOR.replace(floor.max(FLOOR.get())),
        crate::resource::arm(),
    );
    let result = f();
    let reason = REASON.get();
    LAST.set(reason);
    if reason == NONE {
        return result;
    }
    match result {
        Ok(_) => Err(diagnostic(reason)),
        Err(other) => Err(format!("{}\n{other}", diagnostic(reason))),
    }
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
        assert_eq!(DEPTH_NOW.get(), 0);
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

    /// A finding the analysis made is reported after the limit, on its own line (the fail-closed
    /// stand-ins are dropped before this, in `typecheck_request`).
    #[test]
    fn findings_follow_the_limit() {
        let err = check::<()>(|| {
            recurse();
            Err("ANUBIS_SECRET_EXFILTRATION: x".into())
        })
        .unwrap_err();
        let (limit, rest) = err.split_once('\n').unwrap();
        assert!(limit.starts_with("ANUBIS_ANALYSIS_LIMIT"), "{err}");
        assert_eq!(rest, "ANUBIS_SECRET_EXFILTRATION: x");
        assert!(last().is_some());
        assert!(check(|| Ok(())).is_ok());
        assert_eq!(last(), None);
    }

    /// Only the memory budget can be raised, so only its refusal says so.
    #[test]
    fn each_limit_names_itself() {
        assert!(diagnostic(MEMORY).contains("ANUBIS_ANALYSIS_MEMORY_MIB"));
        assert!(diagnostic(MEMORY).starts_with(MEMORY_PREFIX));
        for reason in [STACK, DEPTH] {
            let d = diagnostic(reason);
            assert!(d.starts_with("ANUBIS_ANALYSIS_LIMIT: "), "{d}");
            assert!(!d.contains("ANUBIS_ANALYSIS_MEMORY_MIB"), "{d}");
            assert!(!d.starts_with(MEMORY_PREFIX), "{d}");
            assert!(d.contains("More memory does not change it"), "{d}");
        }
        assert!(diagnostic(DEPTH).contains("4096 nested closure bodies"));
        assert!(diagnostic(STACK).contains("ran out of stack"));
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
        assert_eq!(DEPTH_NOW.get(), 0);
        assert_eq!(FLOOR.get(), 0);
        assert_eq!(check(|| Ok(())), Ok(()));
    }
}
