//! Runs the memcpy hook plan against GTA's real memcpy code: the bytes the
//! mod dumped from the player's GTA5.exe (GTA_MEMCPY_DUMP=<raw file>, placed
//! at its RVA 0x187F840 in a fake image); skipped otherwise. Linux x86-64
//! only: the function is called with the Windows x64 ABI.
#![cfg(all(unix, target_arch = "x86_64"))]

use skate_gta::patch::{entry_patch, find, trampoline, PATCH_LEN};
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

const RVA: usize = 0x187F840;
/// Covers the CPU-feature flag the function reads (rip-relative, ~0x2FC325C).
const IMAGE: usize = 0x3000000;

type Memcpy = unsafe extern "win64" fn(*mut u8, *const u8, usize) -> *mut u8;

static TRAMPOLINE: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static LAST: AtomicUsize = AtomicUsize::new(0);

unsafe extern "win64" fn detour(dst: *mut u8, src: *const u8, len: usize) -> *mut u8 {
    let original: Memcpy = unsafe { std::mem::transmute(TRAMPOLINE.load(SeqCst)) };
    let out = unsafe { original(dst, src, len) };
    CALLS.fetch_add(1, SeqCst);
    LAST.store(len, SeqCst);
    out
}

fn executable(len: usize) -> *mut u8 {
    let p = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    assert_ne!(p, libc::MAP_FAILED);
    p as *mut u8
}

fn check(memcpy: Memcpy, hooked: bool) {
    let source: Vec<u8> = (0..4096u32).map(|i| (i * 7 + 3) as u8).collect();
    let lengths = (0..300).chain([511, 512, 513, 1000, 2047, 2048, 4000, 4096]);
    for len in lengths {
        for offset in [0usize, 1, 3, 8] {
            let mut target = vec![0xEEu8; 4096 + 32];
            let before = CALLS.load(SeqCst);
            let out = unsafe { memcpy(target.as_mut_ptr().add(offset), source.as_ptr(), len) };
            assert_eq!(out, unsafe { target.as_mut_ptr().add(offset) }, "returns the destination");
            assert_eq!(&target[offset..offset + len], &source[..len], "len {len} offset {offset}");
            assert!(target[..offset].iter().all(|&b| b == 0xEE) && target[offset + len..].iter().all(|&b| b == 0xEE));
            if hooked {
                assert_eq!(CALLS.load(SeqCst), before + 1);
                assert_eq!(LAST.load(SeqCst), len);
            }
        }
    }
    // memmove semantics: overlapping copies both ways.
    for (from, to, len) in [(0usize, 5usize, 200usize), (5, 0, 200), (0, 1, 3000), (1, 0, 3000)] {
        let mut buffer: Vec<u8> = (0..4096u32).map(|i| i as u8).collect();
        let mut expected = buffer.clone();
        expected.copy_within(from..from + len, to);
        unsafe { memcpy(buffer.as_mut_ptr().add(to), buffer.as_ptr().add(from), len) };
        assert_eq!(buffer, expected, "overlap {from}->{to} x{len}");
    }
}

#[test]
fn hooked_gta_memcpy_still_copies_and_reports_every_call() {
    let Some(path) = std::env::var_os("GTA_MEMCPY_DUMP") else { return };
    let code = std::fs::read(path).unwrap();
    let base = executable(IMAGE);
    unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), base.add(RVA), code.len()) };
    let image = unsafe { std::slice::from_raw_parts(base, IMAGE) };
    assert_eq!(find(image), Ok(RVA));
    let target = base as usize + RVA;
    let original: Memcpy = unsafe { std::mem::transmute(target) };
    check(original, false);
    // `bt dword [rip+disp], 1` at +0x24 picks the `rep movsb` path (ERMS);
    // GTA copies the bones through it, so run that path too.
    let disp = i32::from_le_bytes(image[RVA + 0x27..RVA + 0x2B].try_into().unwrap());
    let flag = (RVA + 0x2C).wrapping_add(disp as isize as usize);
    assert_eq!(&image[RVA + 0x24..RVA + 0x27], &[0x0F, 0xBA, 0x25]);
    let erms = |on: bool| unsafe { *(base.add(flag) as *mut u32) = if on { u32::MAX } else { 0 } };
    erms(true);
    check(original, false);
    erms(false);

    let entry: [u8; PATCH_LEN] = image[RVA..RVA + PATCH_LEN].try_into().unwrap();
    let plan = trampoline(&entry, target).unwrap();
    let memory = executable(4096);
    unsafe { std::ptr::copy_nonoverlapping(plan.as_ptr(), memory, plan.len()) };
    TRAMPOLINE.store(memory as usize, SeqCst);
    let patch = entry_patch(detour as *const () as usize);
    unsafe { std::ptr::copy_nonoverlapping(patch.as_ptr(), base.add(RVA), PATCH_LEN) };
    check(original, true);
    erms(true);
    check(original, true);
}
