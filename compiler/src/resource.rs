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
//! and, for every cgroup v2 above the process with a `memory.max`, what it has left (its
//! `memory.max` less its `memory.current`, its file cache counted as free: the kernel reclaims
//! clean page cache, active or inactive, before it kills anything). The soft budget is half of it,
//! the hard one two thirds (the process's resident memory runs about a third above what it has
//! allocated, so at the hard budget it still fits). A later request in the same process (the
//! evidence lane re-checks the program it has just checked) takes at least what the first one
//! could use: the memory the process itself holds from the first is not lost to the second.
//!
//! While the request grows, the headroom is read again, from the files themselves each time: every
//! eighth of the reserve it allocates (between 4 and 64 MiB), and before any single allocation that
//! large. When what would be left falls below the reserve (an eighth of the usable memory, at least
//! 128 MiB), the analysis refuses at its next guarded step; below half of it, the process exits at
//! once. Checks run side by side in one scope were each armed with the whole scope's headroom and
//! were killed together by its OOM killer before either refused (review of the checker limits);
//! now whichever reaches its next look first refuses. The reserve applies under
//! `ANUBIS_ANALYSIS_MEMORY_MIB` too: that variable fixes the budget, not the memory that exists.
//! An earlier version took a sixth of the limit, at most 4 GiB: review showed it refused valid
//! programs that fit in the machine several times over (a 600-branch `else if`, a 2000-piece
//! string, closure stress programs), so a check's verdict depended on the machine far more than it
//! had to.
//!
//! The verdict of a program near its budget still depends on the memory the machine has free.
//! `ANUBIS_ANALYSIS_MEMORY_MIB` fixes the soft budget (the hard one is twice it), for a
//! reproducible verdict while the machine has that memory. The budgets bound the checker's
//! analyses (a check request); parsing before it, and the solver, evidence and code generation
//! after it, are not counted against them. Outside a check request nothing is limited and nothing
//! is read. A program that embeds the compiler without installing the allocator has no memory
//! budget, only the stack and depth limits.
use std::alloc::{GlobalAlloc, Layout, System};
#[cfg(target_os = "linux")]
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering::Relaxed};

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Past this, the analysis in progress is refused (0: no request in progress).
static SOFT: AtomicUsize = AtomicUsize::new(0);
/// Past this, the process exits (0: no request in progress).
static HARD: AtomicUsize = AtomicUsize::new(0);
static OVER: AtomicBool = AtomicBool::new(false);
/// Whether it was the reserve, not the budget, that first stopped the request in progress.
static RESERVE_HIT: AtomicBool = AtomicBool::new(false);
/// Past this much allocated, the headroom is read again (0: never).
static NEXT_LOOK: AtomicUsize = AtomicUsize::new(0);
/// How much allocation passes between two readings of the headroom; an allocation at least this
/// large is looked at before it is made.
static LOOK_STEP: AtomicUsize = AtomicUsize::new(MAX_LOOK_STEP);
/// Below this much headroom the request in progress refuses; below half of it, it exits (0: off).
static RESERVE: AtomicUsize = AtomicUsize::new(0);
/// The memory the first request in this process could use (0: none yet).
static FIRST_USABLE: AtomicUsize = AtomicUsize::new(0);
/// The cgroup with the least memory left when the request began: its `memory.current`,
/// `memory.max` and `memory.stat` (NUL-terminated paths, leaked, so the allocator can read them
/// without allocating).
#[cfg(target_os = "linux")]
static CG_CURRENT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
#[cfg(target_os = "linux")]
static CG_MAX: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
#[cfg(target_os = "linux")]
static CG_STAT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
const MIN_LOOK_STEP: usize = 4 * MIB;
const MAX_LOOK_STEP: usize = 64 * MIB;
/// The soft budget of the request in progress, in bytes (for its refusal's message).
static BUDGET: AtomicUsize = AtomicUsize::new(0);
/// What the exits also write to standard output (a machine-readable refusal, when the command line
/// was asked for one), set once before a check: for the hard budget and for the reserve.
static EXIT_REPORT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static EXIT_REPORT_LEN: AtomicUsize = AtomicUsize::new(0);
static RESERVE_REPORT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static RESERVE_REPORT_LEN: AtomicUsize = AtomicUsize::new(0);

const MIB: usize = 1 << 20;
/// The budgets when the platform reports no memory figure at all.
const SOFT_DEFAULT: usize = 4096 * MIB;
const HARD_DEFAULT: usize = 8192 * MIB;

macro_rules! exit_mib {
    () => {
        "18446744073709551557"
    };
}
/// Stands for the soft budget, in MiB, in [`HARD_EXIT_DIAGNOSTIC`] and in the report made from it:
/// the exit writes the number in its place (it cannot format one: it runs inside the allocator).
pub const EXIT_MIB: &str = exit_mib!();
/// The hard budget's exit. Its beginning is the memory refusal's, so a JSON report made from it
/// carries the budget ([`EXIT_MIB`] until the exit writes the number).
pub const HARD_EXIT_DIAGNOSTIC: &str = concat!(
    "ANUBIS_ANALYSIS_LIMIT: the checker's analysis of this program exceeded its memory budget (",
    exit_mib!(),
    " MiB: half the memory free when the check began, or ANUBIS_ANALYSIS_MEMORY_MIB) and kept \
     allocating past its hard budget (two thirds of that memory, or twice the variable) where the \
     analysis could not stop it, so the process stopped at once: the analysis did not complete (a \
     check refuses the program; a verify neither confirms nor refutes the bundle), and nothing it \
     would have written was written. This is a limit of the checker, not a finding about the \
     program: simplify it, or give the check more memory (ANUBIS_ANALYSIS_MEMORY_MIB, in MiB) if \
     the machine has it to spare"
);
/// The reserve's exit: the machine, or the cgroup the check runs in, is nearly out of memory, and
/// no budget of the check's own is the cause, so it names none.
pub const RESERVE_EXIT_DIAGNOSTIC: &str = "ANUBIS_ANALYSIS_LIMIT: the memory left to this check \
(on the machine, or in the memory-capped cgroup it runs in) fell below half the reserve the checker \
keeps free, so the process stopped at once, before the kernel's out-of-memory killer could end it: \
the analysis did not complete (a check refuses the program; a verify neither confirms nor refutes \
the bundle), and nothing it would have written was written. This is a limit of the checker, not a \
finding about the program: other processes are using the memory it needs (raising \
ANUBIS_ANALYSIS_MEMORY_MIB does not change that); run it with more memory free";
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

#[derive(Clone, Copy)]
enum Exit {
    Budget,
    Reserve,
}

fn charge(n: usize) {
    let now = ALLOCATED.fetch_add(n, Relaxed).saturating_add(n);
    // What the machine and the tightest cgroup have left is read again (without allocating: this
    // is the allocator) every LOOK_STEP allocated, and before an allocation that large. Checks run
    // side by side in one scope, or a machine filling up, would otherwise reach the OOM killer
    // before any budget: when what would be left once this allocation is made falls below the
    // reserve, the analysis refuses at its next guarded step; below half of it, at once.
    let look = NEXT_LOOK.load(Relaxed);
    let step = LOOK_STEP.load(Relaxed);
    if look != 0 && (now > look || n >= step) {
        NEXT_LOOK.store(now.saturating_add(step), Relaxed);
        let reserve = RESERVE.load(Relaxed);
        if reserve > 0 {
            if let Some(left) = left_now() {
                let left = left.saturating_sub(n);
                if left < reserve / 2 {
                    exit_on(Exit::Reserve);
                }
                if left < reserve && !OVER.swap(true, Relaxed) {
                    RESERVE_HIT.store(true, Relaxed);
                }
            }
        }
    }
    let soft = SOFT.load(Relaxed);
    if soft != 0 && now > soft {
        OVER.store(true, Relaxed);
        let hard = HARD.load(Relaxed);
        if hard != 0 && now > hard {
            exit_on(Exit::Budget);
        }
    }
}

/// Exit at once with the diagnostic, allocating nothing (this runs inside the allocator).
fn exit_on(why: Exit) -> ! {
    #[cfg(unix)]
    // SAFETY: every write reads static bytes (the diagnostics, and the reports `set_exit_reports`
    // was given, which are leaked, so they live for the process) or a local buffer; `_exit` does
    // not return.
    unsafe {
        let (text, report, len) = match why {
            Exit::Budget => (
                HARD_EXIT_DIAGNOSTIC,
                EXIT_REPORT.load(Relaxed),
                EXIT_REPORT_LEN.load(Relaxed),
            ),
            Exit::Reserve => (
                RESERVE_EXIT_DIAGNOSTIC,
                RESERVE_REPORT.load(Relaxed),
                RESERVE_REPORT_LEN.load(Relaxed),
            ),
        };
        write_all(2, EXIT_NEWLINE);
        write_mib(2, text.as_bytes());
        write_all(2, EXIT_NEWLINE);
        if !report.is_null() && len > 0 {
            write_mib(1, std::slice::from_raw_parts(report, len));
        }
        libc::_exit(1)
    }
    #[cfg(not(unix))]
    {
        let _ = why;
        std::process::abort()
    }
}

/// Write all of `bytes` to `fd` (no allocation).
///
/// # Safety
/// `fd` must be open for writing (or the writes fail harmlessly).
#[cfg(unix)]
unsafe fn write_all(fd: libc::c_int, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = libc::write(fd, bytes.as_ptr().cast(), bytes.len());
        if n <= 0 {
            return;
        }
        bytes = &bytes[n as usize..];
    }
}

/// Write `bytes` to `fd` with the soft budget in MiB in place of every [`EXIT_MIB`] (no allocation).
///
/// # Safety
/// As for [`write_all`].
#[cfg(unix)]
unsafe fn write_mib(fd: libc::c_int, bytes: &[u8]) {
    let mut buf = [0u8; 20];
    let digits = decimal(BUDGET.load(Relaxed) / MIB, &mut buf);
    let mark = EXIT_MIB.as_bytes();
    let mut rest = bytes;
    while let Some(at) = rest.windows(mark.len()).position(|w| w == mark) {
        write_all(fd, &rest[..at]);
        write_all(fd, digits);
        rest = &rest[at + mark.len()..];
    }
    write_all(fd, rest);
}

/// The decimal digits of `n`, written into `buf`.
fn decimal(mut n: usize, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            return &buf[i..];
        }
    }
}

/// Declare that [`CountingAlloc`] is the global allocator (the command line, at start).
pub fn install() {
    INSTALLED.store(true, Relaxed);
}

/// What the exits also write to standard output (the command line's `--message-format json`
/// refusals, made from [`HARD_EXIT_DIAGNOSTIC`] and [`RESERVE_EXIT_DIAGNOSTIC`]), so a consumer
/// reading the stream sees a verdict rather than nothing. [`EXIT_MIB`] in the first is replaced by
/// the budget.
pub fn set_exit_reports(budget: &'static [u8], reserve: &'static [u8]) {
    EXIT_REPORT_LEN.store(0, Relaxed);
    EXIT_REPORT.store(budget.as_ptr().cast_mut(), Relaxed);
    EXIT_REPORT_LEN.store(budget.len(), Relaxed);
    RESERVE_REPORT_LEN.store(0, Relaxed);
    RESERVE_REPORT.store(reserve.as_ptr().cast_mut(), Relaxed);
    RESERVE_REPORT_LEN.store(reserve.len(), Relaxed);
}

/// The soft budget of the request in progress (or the last one), in MiB.
pub fn soft_budget_mib() -> usize {
    BUDGET.load(Relaxed) / MIB
}

/// The reserve of the request in progress (or the last one), in MiB.
pub(crate) fn reserve_mib() -> usize {
    RESERVE.load(Relaxed) / MIB
}

/// Whether the memory in use has passed the soft budget of the request in progress, or the memory
/// left to the process (read again as it grows) has fallen below the reserve.
pub(crate) fn over_budget() -> bool {
    OVER.load(Relaxed)
}

/// Whether it was the reserve, not the budget, that stopped the request in progress (or the last).
pub(crate) fn reserve_hit() -> bool {
    RESERVE_HIT.load(Relaxed)
}

/// What [`arm`] replaced, for [`disarm`] to put back.
pub(crate) struct Armed {
    soft: usize,
    hard: usize,
    over: bool,
    reserve_hit: bool,
    reserve: usize,
    next_look: usize,
    look_step: usize,
}

/// The budgets of one check request, from what is in use now. Returns what to restore when the
/// request ends (`None`: the allocator is not installed, nothing was changed).
pub(crate) fn arm() -> Option<Armed> {
    if !INSTALLED.load(Relaxed) {
        return None;
    }
    let usable = process_usable();
    let (soft, hard) = budgets(usable);
    BUDGET.store(soft, Relaxed);
    let base = ALLOCATED.load(Relaxed);
    // An eighth of what is usable, at least 128 MiB, is kept free (under
    // ANUBIS_ANALYSIS_MEMORY_MIB too); the headroom is read every eighth of that, 4 to 64 MiB.
    let reserve = usable.map_or(0, |u| (u / 8).max(128 * MIB));
    let step = (reserve / 8).clamp(MIN_LOOK_STEP, MAX_LOOK_STEP);
    #[cfg(target_os = "linux")]
    match cgroup_scan() {
        Some((_, dir)) => {
            set_path(&CG_CURRENT, &dir.join("memory.current"));
            set_path(&CG_MAX, &dir.join("memory.max"));
            set_path(&CG_STAT, &dir.join("memory.stat"));
        }
        None => {
            CG_CURRENT.store(std::ptr::null_mut(), Release);
            CG_MAX.store(std::ptr::null_mut(), Release);
            CG_STAT.store(std::ptr::null_mut(), Release);
        }
    }
    let prev = Armed {
        soft: SOFT.load(Relaxed),
        hard: HARD.load(Relaxed),
        over: OVER.load(Relaxed),
        reserve_hit: RESERVE_HIT.load(Relaxed),
        reserve: RESERVE.load(Relaxed),
        next_look: NEXT_LOOK.load(Relaxed),
        look_step: LOOK_STEP.load(Relaxed),
    };
    OVER.store(false, Relaxed);
    RESERVE_HIT.store(false, Relaxed);
    RESERVE.store(reserve, Relaxed);
    LOOK_STEP.store(step, Relaxed);
    NEXT_LOOK.store(base.saturating_add(step), Relaxed);
    SOFT.store(base.saturating_add(soft), Relaxed);
    HARD.store(base.saturating_add(hard), Relaxed);
    Some(prev)
}

/// Put back what `arm` replaced (after the outermost request: no budget, no reserve, no reading).
pub(crate) fn disarm(prev: Armed) {
    NEXT_LOOK.store(prev.next_look, Relaxed);
    LOOK_STEP.store(prev.look_step, Relaxed);
    RESERVE.store(prev.reserve, Relaxed);
    SOFT.store(prev.soft, Relaxed);
    HARD.store(prev.hard, Relaxed);
    OVER.store(prev.over, Relaxed);
    RESERVE_HIT.store(prev.reserve_hit, Relaxed);
}

/// The memory a request can use: what is usable now, and at least what the first request in this
/// process could use. The memory the process holds from an earlier request (its heap, freed but
/// not returned to the system) is counted as used by the system and by its cgroup, yet it is this
/// process's to reuse: without this, the evidence lane's re-check of a program `check` had just
/// passed was armed with a fraction of the budget and refused it (seventh review of the checker
/// limits, N1). What is really left is still read at every look, against the reserve.
fn process_usable() -> Option<usize> {
    let first = FIRST_USABLE.load(Relaxed);
    match usable_memory() {
        Some(now) => {
            if first == 0 {
                FIRST_USABLE.store(now, Relaxed);
            }
            Some(now.max(first))
        }
        None => (first != 0).then_some(first),
    }
}

fn budgets(usable: Option<usize>) -> (usize, usize) {
    if let Some(mib) = std::env::var("ANUBIS_ANALYSIS_MEMORY_MIB")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|m| *m > 0)
    {
        let soft = mib.saturating_mul(MIB);
        return (soft, soft.saturating_mul(2));
    }
    match usable {
        Some(free) => (free / 2, free / 3 * 2),
        None => (SOFT_DEFAULT, HARD_DEFAULT),
    }
}

/// The memory this process can use now: what the system has available, and at most what every
/// cgroup above the process with a `memory.max` has left.
fn usable_memory() -> Option<usize> {
    let system = available_memory().or_else(physical_memory);
    match (system, cgroup_left()) {
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

#[cfg(target_os = "linux")]
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

/// The least memory any cgroup v2 on the path from this process's cgroup to the root has left: its
/// `memory.max` less its `memory.current`, its file cache counted as free (a check run in a
/// memory-capped scope must refuse before the scope's OOM killer ends it, whatever else runs in
/// the scope).
#[cfg(target_os = "linux")]
fn cgroup_left() -> Option<usize> {
    cgroup_scan().map(|(left, _)| left)
}

/// The cgroup with the least memory left on the path from this process's to the root: what it has
/// left, and its directory.
#[cfg(target_os = "linux")]
fn cgroup_scan() -> Option<(usize, std::path::PathBuf)> {
    let own = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let rel = own.lines().find_map(|l| l.strip_prefix("0::"))?.trim();
    let mut dir = std::path::PathBuf::from("/sys/fs/cgroup");
    dir.push(rel.trim_start_matches('/'));
    let mut best: Option<(usize, std::path::PathBuf)> = None;
    loop {
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
        if let Some(max) = read("memory.max").and_then(|v| v.trim().parse::<usize>().ok()) {
            let current = read("memory.current")
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let cache = read("memory.stat").map_or(0, |s| file_cache(s.as_bytes()));
            let here = max.saturating_sub(current.saturating_sub(cache));
            if best.as_ref().is_none_or(|(l, _)| here < *l) {
                best = Some((here, dir.clone()));
            }
        }
        if dir == std::path::Path::new("/sys/fs/cgroup") || !dir.pop() {
            break;
        }
    }
    best
}

/// Point `slot` at a NUL-terminated copy of `path`, leaked (kept when it is the same path).
#[cfg(target_os = "linux")]
fn set_path(slot: &AtomicPtr<u8>, path: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;
    let bytes = path.as_os_str().as_bytes();
    let cur = slot.load(Acquire);
    // SAFETY: a non-null slot points at a leaked NUL-terminated copy (below).
    if !cur.is_null() && unsafe { std::ffi::CStr::from_ptr(cur.cast()) }.to_bytes() == bytes {
        return;
    }
    let mut v = bytes.to_vec();
    v.push(0);
    slot.store(Box::leak(v.into_boxed_slice()).as_mut_ptr(), Release);
}

/// What the machine and the tightest cgroup have left now, read without allocating (from inside
/// the allocator). The cgroup's file cache is read each time too: counted once at the start, it
/// overstated the headroom by as much of it as the kernel had since reclaimed, and checks run side
/// by side in a scope holding page cache were killed together again (seventh review, N3).
#[cfg(target_os = "linux")]
fn left_now() -> Option<usize> {
    let mut buf = [0u8; 1024];
    // SAFETY: a literal NUL-terminated path; `read_raw` writes only into `buf`.
    let meminfo = unsafe { read_raw(c"/proc/meminfo".as_ptr().cast(), &mut buf) };
    let mut left = meminfo.and_then(|n| {
        let text = &buf[..n];
        let at = text.windows(13).position(|w| w == b"MemAvailable:")?;
        let digits = text[at + 13..].iter().skip_while(|c| **c == b' ');
        leading_number(digits.copied()).map(|kib| kib.saturating_mul(1024))
    });
    let (cur, max, stat) = (
        CG_CURRENT.load(Acquire),
        CG_MAX.load(Acquire),
        CG_STAT.load(Acquire),
    );
    if !cur.is_null() && !max.is_null() {
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        let mut s = [0u8; 4096];
        // SAFETY: all three point at leaked NUL-terminated paths (`set_path`), or `stat` is null.
        let (c, m) = unsafe { (read_raw(cur, &mut a), read_raw(max, &mut b)) };
        let cache = if stat.is_null() {
            0
        } else {
            // SAFETY: as above.
            unsafe { read_raw(stat, &mut s) }.map_or(0, |n| file_cache(&s[..n]))
        };
        if let (Some(c), Some(m)) = (c, m) {
            if let (Some(c), Some(m)) = (
                leading_number(a[..c].iter().copied()),
                leading_number(b[..m].iter().copied()),
            ) {
                let here = m.saturating_sub(c.saturating_sub(cache));
                left = Some(left.map_or(here, |l| l.min(here)));
            }
        }
    }
    left
}

#[cfg(not(target_os = "linux"))]
fn left_now() -> Option<usize> {
    None
}

/// Read a file into `buf` with raw system calls (no allocation), up to the buffer's size. Returns
/// how many bytes.
///
/// # Safety
/// `path` must point at a NUL-terminated path.
#[cfg(target_os = "linux")]
unsafe fn read_raw(path: *const u8, buf: &mut [u8]) -> Option<usize> {
    let fd = libc::open(path.cast(), libc::O_RDONLY | libc::O_CLOEXEC);
    if fd < 0 {
        return None;
    }
    let mut n = 0;
    while n < buf.len() {
        let r = libc::read(fd, buf[n..].as_mut_ptr().cast(), buf.len() - n);
        if r <= 0 {
            break;
        }
        n += r as usize;
    }
    libc::close(fd);
    (n > 0).then_some(n)
}

/// The decimal number at the start of `bytes`.
#[cfg(target_os = "linux")]
fn leading_number(bytes: impl Iterator<Item = u8>) -> Option<usize> {
    let mut v: usize = 0;
    let mut any = false;
    for c in bytes {
        if !c.is_ascii_digit() {
            break;
        }
        v = v.checked_mul(10)?.checked_add(usize::from(c - b'0'))?;
        any = true;
    }
    any.then_some(v)
}

/// The value of the line of a cgroup's `memory.stat` that begins with `key` (no allocation).
#[cfg(target_os = "linux")]
fn stat_field(stat: &[u8], key: &[u8]) -> Option<usize> {
    let mut rest = stat;
    loop {
        if rest.starts_with(key) {
            return leading_number(rest[key.len()..].iter().copied());
        }
        let nl = rest.iter().position(|c| *c == b'\n')?;
        rest = &rest[nl + 1..];
    }
}

/// The file cache in a cgroup's `memory.stat`, active and inactive: clean page cache the kernel
/// reclaims before it kills anything. Counting only the inactive part refused valid programs in a
/// scope holding page cache read twice (seventh review of the checker limits, N2).
#[cfg(target_os = "linux")]
fn file_cache(stat: &[u8]) -> usize {
    stat_field(stat, b"active_file ")
        .unwrap_or(0)
        .saturating_add(stat_field(stat, b"inactive_file ").unwrap_or(0))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn cgroup_left() -> Option<usize> {
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
    fn decimal_writes_digits() {
        let mut buf = [0u8; 20];
        assert_eq!(decimal(0, &mut buf), b"0");
        assert_eq!(decimal(1522, &mut buf), b"1522");
        assert_eq!(
            decimal(usize::MAX, &mut buf),
            usize::MAX.to_string().as_bytes()
        );
    }

    #[test]
    fn the_hard_exit_names_its_budget() {
        assert!(HARD_EXIT_DIAGNOSTIC.starts_with(crate::middle::analysis_limit_memory_prefix()));
        assert!(HARD_EXIT_DIAGNOSTIC.contains(EXIT_MIB));
        assert!(EXIT_MIB.parse::<u64>().is_ok());
        assert!(!RESERVE_EXIT_DIAGNOSTIC.starts_with(crate::middle::analysis_limit_memory_prefix()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn mem_available_reads_the_meminfo_line() {
        let meminfo = "MemTotal:       32000000 kB\nMemFree:         1000000 kB\n\
                       MemAvailable:   16000000 kB\nBuffers:          100000 kB\n";
        assert_eq!(mem_available(meminfo), Some(16_000_000 * 1024));
        assert_eq!(mem_available("MemTotal: 5 kB\n"), None);
        assert_eq!(mem_available("MemAvailable: lots\n"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn leading_number_reads_digits() {
        assert_eq!(
            leading_number(b"3221225472\n".iter().copied()),
            Some(3221225472)
        );
        assert_eq!(leading_number(b"max\n".iter().copied()), None);
        assert_eq!(leading_number(b"".iter().copied()), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn left_now_reads_the_machine() {
        assert!(left_now().is_some_and(|l| l > 0));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn file_cache_counts_active_and_inactive_file() {
        let stat = b"anon 1000\nfile 5000\nactive_file 3000\ninactive_file 2000\n";
        assert_eq!(file_cache(stat), 5000);
        assert_eq!(file_cache(b"anon 1\n"), 0);
        // A key is matched at the start of a line only.
        assert_eq!(stat_field(b"inactive_file 7\n", b"active_file "), None);
        assert_eq!(stat_field(b"x 1\nactive_file 9", b"active_file "), Some(9));
    }
}
