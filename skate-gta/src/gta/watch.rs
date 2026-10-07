//! Pose override: right after GTA writes the player ped's last bone matrix,
//! the Skate 3 pose is written over the game's.
//!
//! Two ways to notice that write:
//! - `Mode::Hook`: GTA copies the bones with its memcpy, which is hooked
//!   (`hook.rs`); a copy covering the last bone byte triggers the pose.
//! - `Mode::Guard`: guard pages and a vectored exception handler that
//!   single-steps every access to the page. Slow (GTA copies the array byte
//!   by byte), but for a few frames it also records who touches the
//!   matrices (code address, callers).
use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering::SeqCst},
};
use windows_sys::Win32::{
    Foundation::{EXCEPTION_SINGLE_STEP, STATUS_GUARD_PAGE_VIOLATION},
    System::{
        Diagnostics::{
            Debug::{
                AddVectoredExceptionHandler, RtlLookupFunctionEntry, RtlVirtualUnwind, CONTEXT, EXCEPTION_POINTERS,
                UNW_FLAG_NHANDLER,
            },
        },
        Memory::{VirtualProtect, PAGE_GUARD, PAGE_PROTECTION_FLAGS, PAGE_READWRITE},
        Threading::GetCurrentThreadId,
    },
};

const CONTINUE_EXECUTION: i32 = -1;
const CONTINUE_SEARCH: i32 = 0;
const TRAP_FLAG: u32 = 0x100;
const PAGE: usize = 0x1000;
const MATRIX: usize = 0x40;
const MAX_BONES: usize = 1024;
pub const RECORDS: usize = 8192;
pub const CALLERS: usize = 6;

const STEP_REARM: u8 = 1;
const STEP_APPLY: u8 = 2;

/// Pending single-step work per thread, without TLS or allocation (the
/// handler runs inside arbitrary game code).
const THREADS: usize = 256;
static STEP_THREAD: [AtomicU32; THREADS] = [const { AtomicU32::new(0) }; THREADS];
static STEP_FLAGS: [AtomicU32; THREADS] = [const { AtomicU32::new(0) }; THREADS];
/// Last recorded (code address, bone) per thread: a byte-wise copy of one
/// matrix is recorded once, not 64 times.
static LAST_RECORD: [AtomicU64; THREADS] = [const { AtomicU64::new(0) }; THREADS];

fn step_slot() -> Option<usize> {
    let me = unsafe { GetCurrentThreadId() };
    let start = me as usize % THREADS;
    for k in 0..THREADS {
        let i = (start + k) % THREADS;
        let owner = STEP_THREAD[i].load(SeqCst);
        if owner == me {
            return Some(i);
        }
        if owner == 0 && STEP_THREAD[i].compare_exchange(0, me, SeqCst, SeqCst).is_ok() {
            return Some(i);
        }
    }
    None
}

/// Guard faults allowed per armed frame before the watch gives up on it
/// (a byte-wise copy through the guarded pages would otherwise stall).
const FRAME_BUDGET: u32 = 20000;
static FRAME_FAULTS: AtomicU32 = AtomicU32::new(0);
static OVER_BUDGET: AtomicU32 = AtomicU32::new(0);

/// Which bone array an access touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Array {
    /// Entity-relative matrices (ScriptHookVDotNet's "global" matrices).
    Objects,
    /// Parent-relative matrices.
    Locals,
}

#[derive(Clone, Copy, Debug)]
pub struct Access {
    pub frame: u32,
    pub thread: u32,
    pub write: bool,
    pub array: Array,
    pub offset: usize,
    pub rip: u64,
    pub callers: [u64; CALLERS],
}

struct Slot {
    frame: AtomicU32,
    thread: AtomicU32,
    kind: AtomicU32,
    offset: AtomicU64,
    rip: AtomicU64,
    callers: [AtomicU64; CALLERS],
}

#[allow(clippy::declare_interior_mutable_const)]
const EMPTY: Slot = Slot {
    frame: AtomicU32::new(0),
    thread: AtomicU32::new(0),
    kind: AtomicU32::new(0),
    offset: AtomicU64::new(0),
    rip: AtomicU64::new(0),
    callers: [const { AtomicU64::new(0) }; CALLERS],
};

static SLOTS: [Slot; RECORDS] = [EMPTY; RECORDS];
static NEXT: AtomicUsize = AtomicUsize::new(0);

static OBJECTS: AtomicUsize = AtomicUsize::new(0);
static LOCALS: AtomicUsize = AtomicUsize::new(0);
static COUNT: AtomicUsize = AtomicUsize::new(0);
static ARMED: AtomicBool = AtomicBool::new(false);
static PROBING: AtomicBool = AtomicBool::new(false);
static OVERRIDE: AtomicBool = AtomicBool::new(false);
static FRAME: AtomicU32 = AtomicU32::new(0);
static FAULTS: AtomicU32 = AtomicU32::new(0);
static APPLIED: AtomicU32 = AtomicU32::new(0);
static HANDLER: AtomicUsize = AtomicUsize::new(0);
static WATCHING: AtomicBool = AtomicBool::new(false);
/// This mod's own code range: its accesses never trigger the override.
static OWN_START: AtomicUsize = AtomicUsize::new(0);
static OWN_END: AtomicUsize = AtomicUsize::new(0);
static GAME_START: AtomicUsize = AtomicUsize::new(0);
static GAME_END: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Hook,
    Guard,
}

static HOOKED: AtomicBool = AtomicBool::new(false);
/// Last byte of the bone matrices (0 = none): a copy covering it ends the
/// game's bone write.
static LAST_BYTE: AtomicUsize = AtomicUsize::new(0);
/// Hooked copies since the last read: covering the last bone / touching the array.
static COPIES_LAST: AtomicU32 = AtomicU32::new(0);
static COPIES_ANY: AtomicU32 = AtomicU32::new(0);

/// Rows X, Y, Z, position (xyz each) per bone; NaN marks "leave as is".
struct PoseBuffer(UnsafeCell<[[f32; 12]; MAX_BONES]>);
unsafe impl Sync for PoseBuffer {}
static POSE: PoseBuffer = PoseBuffer(UnsafeCell::new([[f32::NAN; 12]; MAX_BONES]));
static POSE_BUSY: AtomicBool = AtomicBool::new(false);
static POSE_READY: AtomicBool = AtomicBool::new(false);

fn ranges() -> [(usize, usize, Array); 2] {
    let len = COUNT.load(SeqCst) * MATRIX;
    [(OBJECTS.load(SeqCst), len, Array::Objects), (LOCALS.load(SeqCst), len, Array::Locals)]
}

fn page_span(start: usize, len: usize) -> (usize, usize) {
    let first = start & !(PAGE - 1);
    let end = (start + len + PAGE - 1) & !(PAGE - 1);
    (first, end - first)
}

fn on_pages(address: usize) -> bool {
    ranges().iter().any(|&(start, len, _)| {
        let (first, span) = page_span(start, len);
        start != 0 && address >= first && address < first + span
    })
}

/// Unguarding covers both arrays; guarding covers both only while probing,
/// otherwise just the page(s) of the last bone, the one the override needs.
fn protect(guard: bool) {
    let [(objects, len, _), (locals, _, _)] = ranges();
    if objects == 0 || len == 0 {
        return;
    }
    let full = !guard || PROBING.load(SeqCst);
    let spans = if full {
        [page_span(objects, len), if locals != 0 { page_span(locals, len) } else { (0, 0) }]
    } else {
        [page_span(objects + len - MATRIX, MATRIX), (0, 0)]
    };
    let flags: PAGE_PROTECTION_FLAGS = if guard { PAGE_READWRITE | PAGE_GUARD } else { PAGE_READWRITE };
    for (first, span) in spans {
        if first == 0 || span == 0 {
            continue;
        }
        let mut old = 0;
        unsafe { VirtualProtect(first as *const _, span, flags, &mut old) };
    }
}

/// Starts watching a skeleton. `objects`/`locals` are the matrix arrays;
/// `own` is this module's address range.
pub fn start(
    objects: *mut f32,
    locals: *mut f32,
    count: usize,
    override_pose: bool,
    own: (usize, usize),
    game: (usize, usize),
    mode: Mode,
) -> Result<(), String> {
    stop();
    if count == 0 || count > MAX_BONES {
        return Err(format!("unexpected bone count {count}"));
    }
    OWN_START.store(own.0, SeqCst);
    OWN_END.store(own.1, SeqCst);
    GAME_START.store(game.0, SeqCst);
    GAME_END.store(game.1, SeqCst);
    OBJECTS.store(objects as usize, SeqCst);
    LOCALS.store(locals as usize, SeqCst);
    COUNT.store(count, SeqCst);
    OVERRIDE.store(override_pose, SeqCst);
    POSE_READY.store(false, SeqCst);
    NEXT.store(0, SeqCst);
    FRAME.store(0, SeqCst);
    if mode == Mode::Guard && HANDLER.load(SeqCst) == 0 {
        let handle = unsafe { AddVectoredExceptionHandler(1, Some(handler)) };
        if handle.is_null() {
            return Err("AddVectoredExceptionHandler failed".into());
        }
        HANDLER.store(handle as usize, SeqCst);
    }
    WATCHING.store(true, SeqCst);
    HOOKED.store(mode == Mode::Hook, SeqCst);
    if mode == Mode::Hook {
        LAST_BYTE.store(objects as usize + count * MATRIX - 1, SeqCst);
        let image = unsafe { std::slice::from_raw_parts(game.0 as *const u8, game.1 - game.0) };
        super::hook::install(image, after_copy)?;
    }
    Ok(())
}

/// Runs after every GTA memcpy; must stay tiny.
fn after_copy(dst: usize, len: usize) {
    use std::sync::atomic::Ordering::Relaxed;
    let last = LAST_BYTE.load(Relaxed);
    if last == 0 || dst > last || len == 0 {
        return;
    }
    let start = OBJECTS.load(Relaxed);
    if dst.wrapping_add(len) <= start {
        return;
    }
    COPIES_ANY.fetch_add(1, Relaxed);
    if dst.wrapping_add(len) > last {
        COPIES_LAST.fetch_add(1, Relaxed);
        if OVERRIDE.load(Relaxed) && WATCHING.load(Relaxed) {
            apply_pose();
        }
    }
}

/// Hooked copies (covering the last bone, touching the bones at all) since the last call.
pub fn copies() -> (u32, u32) {
    (COPIES_LAST.swap(0, SeqCst), COPIES_ANY.swap(0, SeqCst))
}

/// Stops guarding. The handler stays installed and keeps the last ranges,
/// so a guard re-armed by a racing game thread is still handled.
pub fn stop() {
    WATCHING.store(false, SeqCst);
    ARMED.store(false, SeqCst);
    protect(false);
    LAST_BYTE.store(0, SeqCst);
    HOOKED.store(false, SeqCst);
}

pub fn active() -> bool {
    WATCHING.load(SeqCst)
}

/// Lets the script touch the matrices without faults.
pub fn disarm() {
    if ARMED.swap(false, SeqCst) {
        protect(false);
    }
}

/// Guards the matrices until the next `disarm`; `probe` records accesses.
pub fn arm(frame: u32, probe: bool) {
    if !active() || HOOKED.load(SeqCst) {
        return;
    }
    if PROBING.load(SeqCst) && !probe {
        // The previous frame guarded both arrays; drop those guards first.
        protect(false);
    }
    FRAME.store(frame, SeqCst);
    FRAME_FAULTS.store(0, SeqCst);
    PROBING.store(probe, SeqCst);
    ARMED.store(true, SeqCst);
    protect(true);
}

/// The pose to put over the game's after its last bone write.
pub fn set_pose(rows: impl Iterator<Item = (usize, [f32; 12])>) {
    while POSE_BUSY.swap(true, SeqCst) {
        std::hint::spin_loop();
    }
    let pose = unsafe { &mut *POSE.0.get() };
    for (i, row) in rows {
        if i < MAX_BONES {
            pose[i] = row;
        }
    }
    POSE_BUSY.store(false, SeqCst);
    POSE_READY.store(true, SeqCst);
}

pub fn faults() -> u32 {
    FAULTS.swap(0, SeqCst)
}

/// Frames on which the fault budget ran out since the last call.
pub fn over_budget() -> u32 {
    OVER_BUDGET.swap(0, SeqCst)
}

pub fn watched_objects() -> usize {
    OBJECTS.load(SeqCst)
}

pub fn applied() -> u32 {
    APPLIED.swap(0, SeqCst)
}

pub fn records() -> Vec<Access> {
    let n = NEXT.load(SeqCst).min(RECORDS);
    SLOTS[..n]
        .iter()
        .map(|s| {
            let kind = s.kind.load(SeqCst);
            Access {
                frame: s.frame.load(SeqCst),
                thread: s.thread.load(SeqCst),
                write: kind & 1 != 0,
                array: if kind & 2 != 0 { Array::Locals } else { Array::Objects },
                offset: s.offset.load(SeqCst) as usize,
                rip: s.rip.load(SeqCst),
                callers: std::array::from_fn(|i| s.callers[i].load(SeqCst)),
            }
        })
        .collect()
}

/// Begin and end of the function containing `address`, from the unwind table.
pub fn function_bounds(address: u64) -> Option<(u64, u64)> {
    let mut base = 0u64;
    let entry = unsafe { RtlLookupFunctionEntry(address, &mut base, std::ptr::null_mut()) };
    if entry.is_null() {
        return None;
    }
    let entry = unsafe { &*entry };
    Some((base + entry.BeginAddress as u64, base + entry.EndAddress as u64))
}

fn apply_pose() {
    if !POSE_READY.load(SeqCst) || POSE_BUSY.swap(true, SeqCst) {
        return;
    }
    let objects = OBJECTS.load(SeqCst) as *mut f32;
    let count = COUNT.load(SeqCst);
    let pose = unsafe { &*POSE.0.get() };
    for (i, row) in pose.iter().enumerate().take(count) {
        if row.iter().any(|v| !v.is_finite()) {
            continue;
        }
        unsafe {
            let m = objects.add(i * MATRIX / 4);
            for r in 0..4 {
                for c in 0..3 {
                    *m.add(r * 4 + c) = row[r * 3 + c];
                }
            }
        }
    }
    POSE_BUSY.store(false, SeqCst);
    APPLIED.fetch_add(1, SeqCst);
}

fn walk(context: &CONTEXT) -> [u64; CALLERS] {
    let mut out = [0u64; CALLERS];
    let mut c = *context;
    let (game_start, game_end) = (GAME_START.load(SeqCst) as u64, GAME_END.load(SeqCst) as u64);
    for slot in out.iter_mut() {
        // Unwind only through GTA5.exe code, whose unwind data is known.
        if c.Rip < game_start || c.Rip >= game_end {
            break;
        }
        let mut base = 0u64;
        let entry = unsafe { RtlLookupFunctionEntry(c.Rip, &mut base, std::ptr::null_mut()) };
        if entry.is_null() {
            // Leaf function: the return address is on top of the stack.
            if c.Rsp == 0 {
                break;
            }
            c.Rip = unsafe { *(c.Rsp as *const u64) };
            c.Rsp += 8;
        } else {
            let mut data = std::ptr::null_mut();
            let mut frame = 0u64;
            unsafe {
                RtlVirtualUnwind(UNW_FLAG_NHANDLER, base, c.Rip, entry, &mut c, &mut data, &mut frame, std::ptr::null_mut())
            };
        }
        if c.Rip == 0 {
            break;
        }
        *slot = c.Rip;
    }
    out
}

fn record(context: &CONTEXT, write: bool, array: Array, offset: usize) {
    let i = NEXT.fetch_add(1, SeqCst);
    if i >= RECORDS {
        return;
    }
    let s = &SLOTS[i];
    s.frame.store(FRAME.load(SeqCst), SeqCst);
    s.thread.store(unsafe { GetCurrentThreadId() }, SeqCst);
    s.kind.store(write as u32 | if array == Array::Locals { 2 } else { 0 }, SeqCst);
    s.offset.store(offset as u64, SeqCst);
    s.rip.store(context.Rip, SeqCst);
    for (slot, caller) in s.callers.iter().zip(walk(context)) {
        slot.store(caller, SeqCst);
    }
}

unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let (record_ptr, context) = unsafe { ((*info).ExceptionRecord, &mut *(*info).ContextRecord) };
    let exception = unsafe { &*record_ptr };
    if exception.ExceptionCode == STATUS_GUARD_PAGE_VIOLATION {
        let address = exception.ExceptionInformation[1];
        if !on_pages(address) {
            return CONTINUE_SEARCH;
        }
        FAULTS.fetch_add(1, SeqCst);
        let Some(slot) = step_slot() else {
            // No slot to finish the single step: leave the page unguarded.
            return CONTINUE_EXECUTION;
        };
        let write = exception.ExceptionInformation[0] == 1;
        let mut step = STEP_REARM;
        for (start, len, array) in ranges() {
            if start == 0 || address < start || address >= start + len {
                continue;
            }
            let offset = address - start;
            if PROBING.load(SeqCst) {
                let key = context.Rip ^ ((offset / MATRIX) as u64) << 48 ^ (array as u64) << 60 ^ (write as u64) << 61;
                if LAST_RECORD[slot].swap(key, SeqCst) != key {
                    record(context, write, array, offset);
                }
            }
            let last = len - MATRIX;
            let rip = context.Rip as usize;
            let own = rip >= OWN_START.load(SeqCst) && rip < OWN_END.load(SeqCst);
            if array == Array::Objects && write && offset >= last && !own && OVERRIDE.load(SeqCst) {
                step |= STEP_APPLY;
            }
        }
        // Keep a pending apply if the same instruction faults again.
        STEP_FLAGS[slot].fetch_or(step as u32, SeqCst);
        context.EFlags |= TRAP_FLAG;
        return CONTINUE_EXECUTION;
    }
    if exception.ExceptionCode == EXCEPTION_SINGLE_STEP {
        let Some(slot) = step_slot() else { return CONTINUE_SEARCH };
        let step = STEP_FLAGS[slot].swap(0, SeqCst) as u8;
        if step == 0 {
            return CONTINUE_SEARCH;
        }
        if step & STEP_APPLY != 0 && ARMED.load(SeqCst) {
            protect(false);
            apply_pose();
        }
        if ARMED.load(SeqCst) {
            if FRAME_FAULTS.fetch_add(1, SeqCst) < FRAME_BUDGET {
                protect(true);
            } else if FRAME_FAULTS.load(SeqCst) == FRAME_BUDGET + 1 {
                OVER_BUDGET.fetch_add(1, SeqCst);
            }
        }
        return CONTINUE_EXECUTION;
    }
    CONTINUE_SEARCH
}

/// Human-readable summary of the recorded accesses for the log: who reads
/// and writes the bone matrices, in what order, and the code around them.
pub fn report(records: &[Access], game: (usize, usize)) -> Vec<String> {
    use std::collections::BTreeMap;
    let name = |a: u64| {
        let a = a as usize;
        if a >= game.0 && a < game.1 { format!("GTA5+0x{:X}", a - game.0) } else { format!("0x{a:X}") }
    };
    let bone = |offset: usize| offset / MATRIX;
    let mut lines = vec![format!("Pose watch: {} accesses recorded, GTA5 image 0x{:X}..0x{:X}", records.len(), game.0, game.1)];
    // Per frame: runs of the same code over the same array.
    let mut frames: BTreeMap<u32, Vec<&Access>> = BTreeMap::new();
    for r in records {
        frames.entry(r.frame).or_default().push(r);
    }
    for (frame, list) in &frames {
        let mut runs: Vec<String> = Vec::new();
        let mut i = 0;
        while i < list.len() {
            let a = list[i];
            let mut j = i;
            let (mut lo, mut hi) = (bone(a.offset), bone(a.offset));
            while j + 1 < list.len() && list[j + 1].rip == a.rip && list[j + 1].array == a.array && list[j + 1].write == a.write {
                j += 1;
                lo = lo.min(bone(list[j].offset));
                hi = hi.max(bone(list[j].offset));
            }
            runs.push(format!(
                "{}{} {:?} bones {lo}..{hi} x{} t{}",
                if a.write { "W " } else { "R " },
                name(a.rip),
                a.array,
                j - i + 1,
                a.thread
            ));
            i = j + 1;
            if runs.len() >= 60 {
                runs.push("...".into());
                break;
            }
        }
        lines.push(format!("Pose watch frame {frame}: {}", runs.join(" | ")));
    }
    // Distinct code addresses with their call stacks.
    let mut sites: BTreeMap<(u64, bool, Array), (usize, &Access)> = BTreeMap::new();
    for r in records {
        sites.entry((r.rip, r.write, r.array)).or_insert((0, r)).0 += 1;
    }
    let mut functions: Vec<(u64, u64)> = Vec::new();
    let mut calls: Vec<u64> = Vec::new();
    for ((rip, write, array), (count, sample)) in &sites {
        let bounds = function_bounds(*rip);
        let stack: Vec<String> = sample.callers.iter().take_while(|&&c| c != 0).map(|&c| name(c)).collect();
        lines.push(format!(
            "Pose watch site {} {} {:?} x{count}: function {} | callers {}",
            if *write { "W" } else { "R" },
            name(*rip),
            array,
            bounds.map_or("?".into(), |(b, e)| format!("{}..{}", name(b), name(e))),
            stack.join(" < ")
        ));
        if *write {
            if let Some(b) = bounds {
                if !functions.contains(&b) {
                    functions.push(b);
                }
            } else if !functions.contains(&(*rip - 0x80, *rip + 0x80)) {
                functions.push((*rip - 0x80, *rip + 0x80));
            }
            for &c in sample.callers.iter().take(4).filter(|&&c| c != 0) {
                if !calls.contains(&c) {
                    calls.push(c);
                }
            }
        }
    }
    let in_game = |a: u64| (a as usize) >= game.0 && (a as usize) < game.1;
    let hex = |start: u64, end: u64| -> Vec<String> {
        (start..end)
            .step_by(32)
            .map(|at| {
                let bytes: String = (at..(at + 32).min(end)).map(|p| format!("{:02X}", unsafe { *(p as *const u8) })).collect();
                format!("  {} {bytes}", name(at))
            })
            .collect()
    };
    for (begin, end) in functions {
        if !in_game(begin) || !in_game(end) {
            continue;
        }
        let end = end.min(begin + 0xC00);
        lines.push(format!("Pose watch code {}..{}:", name(begin), name(end)));
        lines.extend(hex(begin, end));
    }
    for ret in calls {
        if in_game(ret) && in_game(ret - 0x30) {
            lines.push(format!("Pose watch call site before {}:", name(ret)));
            lines.extend(hex(ret - 0x30, ret + 0x10));
        }
    }
    lines
}
