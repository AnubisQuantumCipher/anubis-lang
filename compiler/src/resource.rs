//! A counting global allocator, so a check can be bounded in memory as well as in stack.
//!
//! Several of the checker's analyses copy or substitute expressions, and some shapes of valid
//! source make that grow exponentially or quadratically (review of the analysis limits,
//! 2026-09-24). Unbounded, such a check exhausts the machine: the kernel's out-of-memory killer
//! then takes down whatever shares the check's session, the terminal included.
//!
//! The command line installs [`CountingAlloc`] as its global allocator (`tools/anubis`). While a
//! check request is in progress (`middle::analysis_limit::check`), two budgets apply, counted from
//! what was allocated when the request began:
//! - past the soft budget, the analysis in progress is refused at its next guarded step
//!   (`ANUBIS_ANALYSIS_LIMIT`);
//! - past the hard budget (an analysis the guard does not reach kept allocating), the process
//!   prints that diagnostic and exits with status 1 at once.
//!
//! The soft budget is a sixth of the memory available to the process (physical memory, or the
//! smallest cgroup `memory.max` above it, when lower), at most 4 GiB; the hard one is a third, at
//! most 8 GiB (the process's resident memory runs about a third above what it has allocated).
//! The budgets bound the checker's analyses (a check request); parsing before it, and the solver,
//! evidence and code generation after it, are not counted against them.
//! `ANUBIS_ANALYSIS_MEMORY_MIB` sets the soft budget (the hard one is twice it). Outside a check
//! request nothing is limited. A program that embeds the compiler without installing the
//! allocator has no memory budget, only the stack and depth limits.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Past this, the analysis in progress is refused (0: no request in progress).
static SOFT: AtomicUsize = AtomicUsize::new(0);
/// Past this, the process exits (0: no request in progress).
static HARD: AtomicUsize = AtomicUsize::new(0);
static OVER: AtomicBool = AtomicBool::new(false);

const MIB: usize = 1 << 20;
const SOFT_MAX: usize = 4096 * MIB;
const HARD_MAX: usize = 8192 * MIB;
const EXIT_MESSAGE: &[u8] = b"\nANUBIS_ANALYSIS_LIMIT: the checker's analysis of this program exceeded its \
memory budget (ANUBIS_ANALYSIS_MEMORY_MIB); the check did not complete, so the program is refused\n";

/// The system allocator, counting the bytes in use.
pub struct CountingAlloc;

// SAFETY: every call is forwarded unchanged to `System`; the counters only observe sizes.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        // SAFETY: the caller's contract for `alloc` is passed through unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        charge(layout.size());
        // SAFETY: as for `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        ALLOCATED.fetch_sub(layout.size(), Relaxed);
        // SAFETY: `ptr` was allocated by `System` with `layout` (every allocation goes through it).
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() {
            charge(new_size - layout.size());
        } else {
            ALLOCATED.fetch_sub(layout.size() - new_size, Relaxed);
        }
        // SAFETY: as for `dealloc`, and `new_size` is the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn charge(n: usize) {
    let now = ALLOCATED.fetch_add(n, Relaxed).saturating_add(n);
    let soft = SOFT.load(Relaxed);
    if soft != 0 && now > soft {
        OVER.store(true, Relaxed);
        let hard = HARD.load(Relaxed);
        if hard != 0 && now > hard {
            exit_over_budget();
        }
    }
}

/// Exit at once with the diagnostic, allocating nothing (this runs inside the allocator).
fn exit_over_budget() -> ! {
    #[cfg(unix)]
    // SAFETY: `write` reads `EXIT_MESSAGE` only; `_exit` does not return.
    unsafe {
        libc::write(2, EXIT_MESSAGE.as_ptr().cast(), EXIT_MESSAGE.len());
        libc::_exit(1)
    }
    #[cfg(not(unix))]
    std::process::abort()
}

/// Declare that [`CountingAlloc`] is the global allocator (the command line, at start).
pub fn install() {
    INSTALLED.store(true, Relaxed);
}

/// Whether the memory in use has passed the soft budget of the request in progress.
pub(crate) fn over_budget() -> bool {
    OVER.load(Relaxed)
}

/// The budgets of one check request, from what is in use now. Returns what to restore when the
/// request ends (`None`: the allocator is not installed, nothing was changed).
pub(crate) fn arm() -> Option<(usize, usize, bool)> {
    if !INSTALLED.load(Relaxed) {
        return None;
    }
    let (soft, hard) = budgets();
    let base = ALLOCATED.load(Relaxed);
    let prev = (SOFT.load(Relaxed), HARD.load(Relaxed), OVER.load(Relaxed));
    OVER.store(false, Relaxed);
    SOFT.store(base.saturating_add(soft), Relaxed);
    HARD.store(base.saturating_add(hard), Relaxed);
    Some(prev)
}

/// Put back the budgets `arm` replaced.
pub(crate) fn disarm(prev: (usize, usize, bool)) {
    SOFT.store(prev.0, Relaxed);
    HARD.store(prev.1, Relaxed);
    OVER.store(prev.2, Relaxed);
}

fn budgets() -> (usize, usize) {
    if let Some(mib) = std::env::var("ANUBIS_ANALYSIS_MEMORY_MIB")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|m| *m > 0)
    {
        let soft = mib.saturating_mul(MIB);
        return (soft, soft.saturating_mul(2));
    }
    match physical_memory() {
        Some(total) => ((total / 6).min(SOFT_MAX), (total / 3).min(HARD_MAX)),
        None => (SOFT_MAX, HARD_MAX),
    }
}

#[cfg(unix)]
fn physical_memory() -> Option<usize> {
    // SAFETY: `sysconf` only reads system configuration.
    let (pages, page) = unsafe {
        (
            libc::sysconf(libc::_SC_PHYS_PAGES),
            libc::sysconf(libc::_SC_PAGESIZE),
        )
    };
    let physical = (pages > 0 && page > 0).then(|| (pages as usize).saturating_mul(page as usize));
    match (physical, cgroup_limit()) {
        (Some(p), Some(c)) => Some(p.min(c)),
        (p, c) => p.or(c),
    }
}

/// The smallest cgroup v2 `memory.max` on the path from this process's cgroup to the root (a
/// check run in a memory-capped scope must refuse before the scope's OOM killer ends it).
#[cfg(target_os = "linux")]
fn cgroup_limit() -> Option<usize> {
    let own = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let rel = own.lines().find_map(|l| l.strip_prefix("0::"))?.trim();
    let mut dir = std::path::PathBuf::from("/sys/fs/cgroup");
    dir.push(rel.trim_start_matches('/'));
    let mut limit: Option<usize> = None;
    loop {
        if let Ok(v) = std::fs::read_to_string(dir.join("memory.max")) {
            if let Ok(n) = v.trim().parse::<usize>() {
                limit = Some(limit.map_or(n, |l| l.min(n)));
            }
        }
        if dir == std::path::Path::new("/sys/fs/cgroup") || !dir.pop() {
            break;
        }
    }
    limit
}

#[cfg(all(unix, not(target_os = "linux")))]
fn cgroup_limit() -> Option<usize> {
    None
}

#[cfg(not(unix))]
fn physical_memory() -> Option<usize> {
    None
}
