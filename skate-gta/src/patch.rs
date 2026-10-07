//! Byte-level plan of the memcpy hook (see `gta/hook.rs`), platform-free so
//! it can be checked off Windows.

/// Stock MSVC x64 memcpy/memmove entry, as dumped from the player's GTA5.exe:
/// mov r11, rcx / mov r10, rdx / cmp r8, 16 / jbe rel32 / sub rdx, rcx / ...
pub const MEMCPY_SIGNATURE: [u8; 32] = [
    0x4C, 0x8B, 0xD9, 0x4C, 0x8B, 0xD2, 0x49, 0x83, 0xF8, 0x10, 0x0F, 0x86, 0xA9, 0x00, 0x00, 0x00, //
    0x48, 0x2B, 0xD1, 0x73, 0x0F, 0x49, 0x8B, 0xC2, 0x49, 0x03, 0xC0, 0x48, 0x3B, 0xC8, 0x0F, 0x8C,
];
/// Bytes replaced at the function entry: four whole instructions.
pub const PATCH_LEN: usize = 16;
/// The `mov, mov, cmp` moved as they are; the `jbe rel32` after them is rebuilt.
const MOVED: usize = 10;

/// Offset of the single signature match in `image`.
pub fn find(image: &[u8]) -> Result<usize, String> {
    let mut found = image.windows(MEMCPY_SIGNATURE.len()).enumerate().filter(|(_, w)| *w == MEMCPY_SIGNATURE).map(|(i, _)| i);
    let first = found.next().ok_or("GTA's memcpy was not found (different game build?)")?;
    if found.next().is_some() {
        return Err("GTA's memcpy signature is not unique".into());
    }
    Ok(first)
}

/// `jmp [rip+0]` followed by the 64-bit target.
pub fn absolute_jump(to: usize) -> [u8; 14] {
    let mut j = [0xFF, 0x25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    j[6..].copy_from_slice(&(to as u64).to_le_bytes());
    j
}

/// Code that behaves like the replaced bytes at `target`: the moved
/// instructions, the jbe sent to an absolute jump, and a jump back.
/// Position-independent, so it can be copied anywhere.
pub fn trampoline(original: &[u8; PATCH_LEN], target: usize) -> Result<Vec<u8>, String> {
    if original[..MOVED] != MEMCPY_SIGNATURE[..MOVED] || original[MOVED..MOVED + 2] != [0x0F, 0x86] {
        return Err("unexpected memcpy entry bytes".into());
    }
    let jbe = i32::from_le_bytes(original[12..16].try_into().unwrap());
    let jbe_target = (target + PATCH_LEN).wrapping_add(jbe as isize as usize);
    let mut code = Vec::with_capacity(MOVED + 6 + 28);
    code.extend_from_slice(&original[..MOVED]);
    code.extend_from_slice(&[0x0F, 0x86, 14, 0, 0, 0]); // jbe over the next jump
    code.extend_from_slice(&absolute_jump(target + PATCH_LEN));
    code.extend_from_slice(&absolute_jump(jbe_target));
    Ok(code)
}

/// The entry bytes that jump to `detour`.
pub fn entry_patch(detour: usize) -> [u8; PATCH_LEN] {
    let mut patch = [0x90u8; PATCH_LEN];
    patch[..14].copy_from_slice(&absolute_jump(detour));
    patch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trampoline_rebuilds_the_jbe() {
        let mut original = [0u8; PATCH_LEN];
        original.copy_from_slice(&MEMCPY_SIGNATURE[..PATCH_LEN]);
        let code = trampoline(&original, 0x1000).unwrap();
        assert_eq!(&code[..10], &MEMCPY_SIGNATURE[..10]);
        assert_eq!(&code[10..16], &[0x0F, 0x86, 14, 0, 0, 0]);
        assert_eq!(code[16..30], absolute_jump(0x1010));
        // jbe +0xA9 from the end of the patched bytes.
        assert_eq!(code[30..44], absolute_jump(0x1010 + 0xA9));
        assert!(trampoline(&[0x90; PATCH_LEN], 0).is_err());
        assert_eq!(find(&[&[0u8; 5][..], &MEMCPY_SIGNATURE].concat()), Ok(5));
    }
}
