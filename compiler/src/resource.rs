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
//! The budgets come from the memory this process can use when the request begins: the smaller of
//! what the system has available (`MemAvailable`: free memory plus what the kernel can reclaim)
//! and the smallest cgroup v2 `memory.max` above the process. The soft budget is half of it, the
//! hard one two thirds (the process's resident memory runs about a third above what it has
//! allocated, so at the hard budget it still fits). An earlier version took a sixth of the limit,
//! at most 4 GiB: review showed it refused valid programs that fit in the machine several times
//! over (a 600-branch `else if`, a 2000-piece string, closure stress programs), so a check's
//! verdict depended on the machine far more than it had to.
//!
//! The verdict of a program near its budget still depends on the memory the machine has free.
//! `ANUBIS_ANALYSIS_MEMORY_MIB` fixes the soft budget (the hard one is twice it), for a
//! reproducible verdict. The budgets bound the checker's analyses (a check request); parsing before
//! it, and the solver, evidence and code generation after it, are not counted against them, and
//! checks run side by side each take their budget from the memory free when they begin. Outside a
//! check request nothing is limited. A program that embeds the compiler without installing the
//! allocator has no memory budget, only the stack and depth limits.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering::Relaxed};

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Past this, the analysis in progress is refused (0: no request in progress).
static SOFT: AtomicUsize = AtomicUsize::new(0);
/// Past this, the process exits (0: no request in progress).
static HARD: AtomicUsize = AtomicUsize::new(0);
static OVER: AtomicBool = AtomicBool::new(false);
/// The soft budget of the request in progress, in bytes (for its refusal's message).
static BUDGET: AtomicUsize = AtomicUsize::new(0);
/// What the hard exit also writes to standard output (a machine-readable refusal, when the command
/// line was asked for one), set once before a check.
static EXIT_REPORT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static EXIT_REPORT_LEN: AtomicUsize = AtomicUsize::new(0);

const MIB: usize = 1 << 20;
/// The budgets when the platform reports no memory figure at all.
const SOFT_DEFAULT: usize = 4096 * MIB;
const HARD_DEFAULT: usize = 8192 * MIB;
/// The hard exit's refusal. Written from inside the allocator, so it cannot be formatted there.
pub const HARD_EXIT_DIAGNOSTIC: &str = "ANUBIS_ANALYSIS_LIMIT: the checker's analysis of this \
program kept allocating past its hard memory budget (two thirds of the memory free when the check \
began, or twice ANUBIS_ANALYSIS_MEMORY_MIB); the check did not complete, so the program is \
refused. This is a limit of the checker, not a finding about the program: simplify it, or give \
the check more memory (ANUBIS_ANALYSIS_MEMORY_MIB, in MiB) if the machine has it to spare. No \
evidence bundle is written on this exit";
const EXIT_NEWLINE: &[u8] = b"\n";

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
    // SAFETY: `write` reads static bytes only (the diagnostic, and the report `set_exit_report` was
    // given, which is leaked, so it lives for the process); `_exit` does not return.
    unsafe {
        libc::write(2, EXIT_NEWLINE.as_ptr().cast(), 1);
        libc::write(
            2,
            HARD_EXIT_DIAGNOSTIC.as_ptr().cast(),
            HARD_EXIT_DIAGNOSTIC.len(),
        );
        libc::write(2, EXIT_NEWLINE.as_ptr().cast(), 1);
        let report = EXIT_REPORT.load(Relaxed);
        let len = EXIT_REPORT_LEN.load(Relaxed);
        if !report.is_null() && len > 0 {
            libc::write(1, report.cast(), len);
        }
        libc::_exit(1)
    }
    #[cfg(not(unix))]
    std::process::abort()
}

/// Declare that [`CountingAlloc`] is the global allocator (the command line, at start).
pub fn install() {
    INSTALLED.store(true, Relaxed);
}

/// What the hard exit also writes to standard output (the command line's `--message-format json`
/// refusal), so a consumer reading the stream sees a verdict rather than nothing.
pub fn set_exit_report(report: &'static [u8]) {
    EXIT_REPORT_LEN.store(0, Relaxed);
    EXIT_REPORT.store(report.as_ptr().cast_mut(), Relaxed);
    EXIT_REPORT_LEN.store(report.len(), Relaxed);
}

/// The soft budget of the request in progress (or the last one), in MiB.
pub fn soft_budget_mib() -> usize {
    BUDGET.load(Relaxed) / MIB
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
    BUDGET.store(soft, Relaxed);
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
    match usable_memory() {
        Some(free) => (free / 2, free / 3 * 2),
        None => (SOFT_DEFAULT, HARD_DEFAULT),
    }
}

/// The memory this process can use now: what the system has available, and at most the smallest
/// cgroup `memory.max` above the process.
fn usable_memory() -> Option<usize> {
    let system = available_memory().or_else(physical_memory);
    match (system, cgroup_limit()) {
        (Some(s), Some(c)) => Some(s.min(c)),
        (s, c) => s.or(c),
    }
}

/// `MemAvailable` from `/proc/meminfo`, in bytes.
#[cfg(target_os = "linux")]
fn available_memory() -> Option<usize> {
    mem_available(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(not(target_os = "linux"))]
fn available_memory() -> Option<usize> {
    None
}

fn mem_available(meminfo: &str) -> Option<usize> {
    let kib = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemAvailable:"))?
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<usize>()
        .ok()?;
    Some(kib.saturating_mul(1024))
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
    (pages > 0 && page > 0).then(|| (pages as usize).saturating_mul(page as usize))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_available_reads_the_meminfo_line() {
        let meminfo = "MemTotal:       32000000 kB\nMemFree:         1000000 kB\n\
                       MemAvailable:   16000000 kB\nBuffers:          100000 kB\n";
        assert_eq!(mem_available(meminfo), Some(16_000_000 * 1024));
        assert_eq!(mem_available("MemTotal: 5 kB\n"), None);
        assert_eq!(mem_available("MemAvailable: lots\n"), None);
    }
}
