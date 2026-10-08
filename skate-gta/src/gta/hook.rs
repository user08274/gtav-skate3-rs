//! Inline hook on GTA's CRT memcpy. GTA copies the ped's bone matrices into
//! place with it (found with the guard-page watch: `rep movsb` inside this
//! function writes the bone arrays), so right after a copy that covers the
//! player's last bone the Skate 3 pose can be written over the game's.
//!
//! The function is found by its exact first 32 bytes; `patch.rs` plans the
//! bytes. The entry is replaced by an absolute jump to the detour while
//! every other thread is suspended and none is inside the replaced bytes.
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::{
            Debug::{FlushInstructionCache, GetThreadContext, CONTEXT, CONTEXT_CONTROL_AMD64},
            ToolHelp::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32},
        },
        Memory::{
            VirtualAlloc, VirtualProtect, VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, MEM_RESERVE,
            PAGE_EXECUTE_READWRITE, PAGE_GUARD, PAGE_NOACCESS,
        },
        Threading::{
            GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, OpenThread, ResumeThread, SuspendThread,
            THREAD_GET_CONTEXT, THREAD_SUSPEND_RESUME,
        },
    },
};

use crate::patch::{entry_patch, trampoline, MEMCPY_SIGNATURE, PATCH_LEN};

type Memcpy = unsafe extern "C" fn(*mut u8, *const u8, usize) -> *mut u8;

static TARGET: AtomicUsize = AtomicUsize::new(0);
static TRAMPOLINE: AtomicUsize = AtomicUsize::new(0);
/// Called after every hooked copy with (destination, length).
static AFTER: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn detour(dst: *mut u8, src: *const u8, len: usize) -> *mut u8 {
    let original: Memcpy = unsafe { std::mem::transmute(TRAMPOLINE.load(std::sync::atomic::Ordering::Relaxed)) };
    let out = unsafe { original(dst, src, len) };
    let after = AFTER.load(std::sync::atomic::Ordering::Relaxed);
    if after != 0 {
        let after: fn(usize, usize) = unsafe { std::mem::transmute(after) };
        after(dst as usize, len);
    }
    out
}

pub fn installed() -> bool {
    TARGET.load(SeqCst) != 0
}

/// Installs the hook once; `after` runs after every copy.
pub fn install(image: &[u8], after: fn(usize, usize)) -> Result<(), String> {
    AFTER.store(after as usize, SeqCst);
    if installed() {
        return Ok(());
    }
    let target = find_readable(image)?;
    let original: [u8; PATCH_LEN] = unsafe { *(target as *const [u8; PATCH_LEN]) };
    let code = trampoline(&original, target)?;
    let memory = unsafe { VirtualAlloc(std::ptr::null(), 4096, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) } as *mut u8;
    if memory.is_null() {
        return Err("could not allocate the memcpy trampoline".into());
    }
    unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), memory, code.len()) };
    TRAMPOLINE.store(memory as usize, SeqCst);
    let patch = entry_patch(detour as *const () as usize);
    write_code(target, &original, &patch)?;
    TARGET.store(target, SeqCst);
    Ok(())
}

/// The single signature match, looking only at committed readable pages
/// (the image may contain pages that fault on reading).
fn find_readable(image: &[u8]) -> Result<usize, String> {
    let (start, end) = (image.as_ptr() as usize, image.as_ptr() as usize + image.len());
    let mut at = start;
    let mut found = Vec::new();
    while at < end {
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { VirtualQuery(at as *const _, &mut info, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) } == 0 {
            break;
        }
        let region_end = (info.BaseAddress as usize + info.RegionSize).min(end);
        let readable = info.State == MEM_COMMIT && info.Protect & (PAGE_NOACCESS | PAGE_GUARD) == 0 && info.Protect != 0;
        if readable && region_end > at {
            let region = unsafe { std::slice::from_raw_parts(at as *const u8, region_end - at) };
            let mut offset = 0;
            while let Some(i) = region[offset..]
                .windows(MEMCPY_SIGNATURE.len())
                .position(|w| w == MEMCPY_SIGNATURE)
            {
                found.push(at + offset + i);
                offset += i + 1;
            }
        }
        if region_end <= at {
            break;
        }
        at = region_end;
    }
    match found.as_slice() {
        [one] => Ok(*one),
        [] => Err("GTA's memcpy was not found (different game build?)".into()),
        _ => Err(format!("GTA's memcpy signature matched {} times", found.len())),
    }
}

/// Puts the original bytes back (module unload).
pub fn uninstall() {
    let target = TARGET.swap(0, SeqCst);
    if target == 0 {
        return;
    }
    let mut original = [0u8; PATCH_LEN];
    original.copy_from_slice(&MEMCPY_SIGNATURE[..PATCH_LEN]);
    let current: [u8; PATCH_LEN] = unsafe { *(target as *const [u8; PATCH_LEN]) };
    let _ = write_code(target, &current, &original);
}

#[repr(C, align(16))]
struct AlignedContext(CONTEXT);

fn other_threads() -> Vec<u32> {
    let mut out = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return out;
        }
        let (process, me) = (GetCurrentProcessId(), GetCurrentThreadId());
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut more = Thread32First(snapshot, &mut entry) != 0;
        while more {
            if entry.th32OwnerProcessID == process && entry.th32ThreadID != me {
                out.push(entry.th32ThreadID);
            }
            more = Thread32Next(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    out
}

/// Replaces `expected` at `at` with `bytes` while all other threads are
/// suspended and none of them is executing inside the patched bytes.
fn write_code(at: usize, expected: &[u8; PATCH_LEN], bytes: &[u8; PATCH_LEN]) -> Result<(), String> {
    for _attempt in 0..20 {
        let threads = other_threads();
        // Allocate before suspending: a suspended thread may hold the heap lock.
        let mut suspended: Vec<HANDLE> = Vec::with_capacity(threads.len());
        let mut busy = false;
        unsafe {
            for &thread in &threads {
                let handle = OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT, 0, thread);
                if handle.is_null() {
                    continue;
                }
                if SuspendThread(handle) == u32::MAX {
                    CloseHandle(handle);
                    continue;
                }
                suspended.push(handle);
                let mut context: AlignedContext = std::mem::zeroed();
                context.0.ContextFlags = CONTEXT_CONTROL_AMD64;
                if GetThreadContext(handle, &mut context.0) != 0 {
                    let rip = context.0.Rip as usize;
                    if rip > at && rip < at + PATCH_LEN {
                        busy = true;
                    }
                }
            }
            let mut result = Ok(());
            if !busy {
                let current = &*(at as *const [u8; PATCH_LEN]);
                if current != expected {
                    result = Err("GTA's memcpy changed before patching".to_string());
                } else {
                    let mut old = 0;
                    if VirtualProtect(at as *const _, PATCH_LEN, PAGE_EXECUTE_READWRITE, &mut old) == 0 {
                        result = Err("could not unprotect GTA's memcpy".to_string());
                    } else {
                        std::ptr::copy_nonoverlapping(bytes.as_ptr(), at as *mut u8, PATCH_LEN);
                        VirtualProtect(at as *const _, PATCH_LEN, old, &mut old);
                        FlushInstructionCache(GetCurrentProcess(), at as *const _, PATCH_LEN);
                    }
                }
            }
            for &handle in &suspended {
                ResumeThread(handle);
                CloseHandle(handle);
            }
            if !busy {
                return result;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    Err("a game thread kept running inside memcpy's first bytes".into())
}
