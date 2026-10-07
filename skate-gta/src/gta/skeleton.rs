//! A ped's live skeleton in GTA memory (the same path ScriptHookVDotNet's
//! EntityBone uses): entity -> fragInst (virtual call, offset found by code
//! signature) -> frag cache entry -> crSkeleton. Entity-relative bone
//! matrices written each frame change how the ped is drawn.
use super::shv;
use crate::{coords::GtaVec, pose::BonePose};
use std::sync::OnceLock;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleA;

const FRAG_CACHE: usize = 0x68;
const FRAG_TYPE: usize = 0x78;
const CACHE_SKELETON: usize = 0x178;
const SKELETON_DATA: usize = 0x00;
const SKELETON_GLOBALS: usize = 0x18;
const SKELETON_COUNT: usize = 0x20;
const DATA_BONES: usize = 0x20;
const BONE_STRIDE: usize = 0x50;
const BONE_PARENT: usize = 0x32;
const MATRIX_BYTES: usize = 0x40;

/// `cmp`/`ja` around the entity fragInst getter call; its disp8 is the vtable offset.
const FRAG_INST_SIGNATURE: [u8; 29] = [
    0x0F, 0x84, 0x8F, 0x00, 0x00, 0x00, 0x8A, 0x48, 0x28, 0x80, 0xE9, 0x02, 0x80, 0xF9, 0x03, 0x0F,
    0x87, 0x80, 0x00, 0x00, 0x00, 0x48, 0x8B, 0x10, 0x48, 0x8B, 0xC8, 0xFF, 0x52,
];

fn module_image() -> Option<&'static [u8]> {
    unsafe {
        let base = GetModuleHandleA(std::ptr::null()) as *const u8;
        if base.is_null() {
            return None;
        }
        let nt = base.add(*(base.add(0x3C) as *const u32) as usize);
        let size = *(nt.add(0x50) as *const u32) as usize;
        Some(std::slice::from_raw_parts(base, size))
    }
}

fn frag_inst_offset() -> Option<usize> {
    static OFFSET: OnceLock<Option<usize>> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        let image = module_image()?;
        let at = image.windows(FRAG_INST_SIGNATURE.len()).position(|w| w == FRAG_INST_SIGNATURE)?;
        let disp = *image.get(at + FRAG_INST_SIGNATURE.len())? as i8;
        (disp > 0).then_some(disp as usize)
    })
}

fn plausible(p: *const u8) -> bool {
    (p as usize) > 0x10000 && (p as usize) % 8 == 0
}

unsafe fn read_ptr(base: *const u8, offset: usize) -> *const u8 {
    unsafe { *(base.add(offset) as *const *const u8) }
}

pub struct PedSkeleton {
    globals: *mut f32,
    pub parents: Vec<i32>,
}

impl PedSkeleton {
    pub fn find(ped: i32) -> Result<Self, String> {
        let offset = frag_inst_offset().ok_or("this GTA build's fragInst signature was not found")?;
        let entity = shv::handle_address(ped) as *const u8;
        if !plausible(entity) {
            return Err("ped has no game object".into());
        }
        unsafe {
            let vtable = read_ptr(entity, 0);
            if !plausible(vtable) {
                return Err("ped object has no vtable".into());
            }
            let getter: unsafe extern "C" fn(*const u8) -> *const u8 = std::mem::transmute(read_ptr(vtable, offset));
            let frag = getter(entity);
            if !plausible(frag) || !plausible(read_ptr(frag, FRAG_TYPE)) {
                return Err("ped has no fragment instance".into());
            }
            let cache = read_ptr(frag, FRAG_CACHE);
            if !plausible(cache) {
                return Err("ped fragment has no cache entry".into());
            }
            let skeleton = read_ptr(cache, CACHE_SKELETON);
            if !plausible(skeleton) {
                return Err("ped has no skeleton".into());
            }
            let data = read_ptr(skeleton, SKELETON_DATA);
            let globals = read_ptr(skeleton, SKELETON_GLOBALS) as *mut f32;
            let count = *(skeleton.add(SKELETON_COUNT) as *const i32);
            if !plausible(data) || !plausible(globals as *const u8) || !(1..=1024).contains(&count) {
                return Err(format!("ped skeleton looks invalid (bones {count})"));
            }
            let bones = read_ptr(data, DATA_BONES);
            if !plausible(bones) {
                return Err("ped skeleton has no bone data".into());
            }
            let parents = (0..count as usize)
                .map(|i| {
                    let p = *(bones.add(i * BONE_STRIDE + BONE_PARENT) as *const u16);
                    if p == 0xFFFF { -1 } else { p as i32 }
                })
                .collect();
            Ok(Self { globals, parents })
        }
    }

    pub fn len(&self) -> usize {
        self.parents.len()
    }

    /// Entity-relative matrix rows: X axis, Y axis, Z axis, position.
    pub fn read(&self) -> Vec<BonePose> {
        (0..self.len())
            .map(|i| unsafe {
                let m = self.globals.add(i * MATRIX_BYTES / 4);
                let row = |r: usize| GtaVec::new(*m.add(r * 4), *m.add(r * 4 + 1), *m.add(r * 4 + 2));
                BonePose { axes: [row(0), row(1), row(2)], position: row(3) }
            })
            .collect()
    }

    pub fn write(&self, pose: &[BonePose]) {
        for (i, bone) in pose.iter().enumerate().take(self.len()) {
            let values = [bone.axes[0], bone.axes[1], bone.axes[2], bone.position];
            if values.iter().any(|v| !(v.x.is_finite() && v.y.is_finite() && v.z.is_finite())) {
                continue;
            }
            unsafe {
                let m = self.globals.add(i * MATRIX_BYTES / 4);
                for (r, v) in values.iter().enumerate() {
                    *m.add(r * 4) = v.x;
                    *m.add(r * 4 + 1) = v.y;
                    *m.add(r * 4 + 2) = v.z;
                }
            }
        }
    }
}
