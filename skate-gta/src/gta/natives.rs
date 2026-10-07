//! Typed wrappers for the natives this mod uses. Hashes and signatures come
//! from alloc8or/gta5-nativedb-data.
use super::shv::invoke;
use crate::coords::GtaVec;
use std::ffi::CString;

pub type Entity = i32;

/// ScriptHookV's Vector3: three floats, each in an 8-byte slot.
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
struct NVector3 {
    x: f32,
    _p0: u32,
    y: f32,
    _p1: u32,
    z: f32,
    _p2: u32,
}

trait Word {
    fn word(self) -> u64;
}
impl Word for i32 {
    fn word(self) -> u64 {
        self as u32 as u64
    }
}
impl Word for u32 {
    fn word(self) -> u64 {
        self as u64
    }
}
impl Word for f32 {
    fn word(self) -> u64 {
        self.to_bits() as u64
    }
}
impl Word for bool {
    fn word(self) -> u64 {
        self as u64
    }
}
impl<T> Word for *const T {
    fn word(self) -> u64 {
        self as u64
    }
}
impl<T> Word for *mut T {
    fn word(self) -> u64 {
        self as u64
    }
}

macro_rules! call {
    ($hash:expr $(, $arg:expr)* $(,)?) => {
        unsafe { invoke($hash, &[$(Word::word($arg)),*]) }
    };
}

fn ret_i32(p: *const u64) -> i32 {
    unsafe { *(p as *const i32) }
}
fn ret_bool(p: *const u64) -> bool {
    ret_i32(p) != 0
}
fn ret_f32(p: *const u64) -> f32 {
    unsafe { *(p as *const f32) }
}
fn ret_vec(p: *const u64) -> GtaVec {
    let v = unsafe { *(p as *const NVector3) };
    GtaVec::new(v.x, v.y, v.z)
}

pub fn player_ped_id() -> Entity {
    ret_i32(call!(0xD80958FC74E988A6))
}
pub fn get_entity_coords(e: Entity) -> GtaVec {
    ret_vec(call!(0x3FEF770D40960D5A, e, true))
}
pub fn get_entity_heading(e: Entity) -> f32 {
    ret_f32(call!(0xE83D4F9BA2A38914, e))
}
pub fn does_entity_exist(e: Entity) -> bool {
    ret_bool(call!(0x7239B21A38F536BA, e))
}
pub fn is_entity_dead(e: Entity) -> bool {
    ret_bool(call!(0x5F9532F3B5CC2551, e, false))
}
pub fn is_ped_in_any_vehicle(e: Entity) -> bool {
    ret_bool(call!(0x997ABD671D25CA0B, e, false))
}
pub fn set_entity_coords_no_offset(e: Entity, p: GtaVec) {
    call!(0x239A3351AC1DA385, e, p.x, p.y, p.z, false, false, false);
}
pub fn set_entity_coords(e: Entity, p: GtaVec) {
    call!(0x06843DA7060A026B, e, p.x, p.y, p.z, false, false, false, false);
}
pub fn set_entity_quaternion(e: Entity, q: [f32; 4]) {
    call!(0x77B21BE7AC540F07, e, q[0], q[1], q[2], q[3]);
}
pub fn freeze_entity_position(e: Entity, toggle: bool) {
    call!(0x428CA6DBD1094446, e, toggle);
}
pub fn set_entity_collision(e: Entity, toggle: bool, keep_physics: bool) {
    call!(0x1A9205C1B9EE827F, e, toggle, keep_physics);
}
pub fn set_entity_as_mission_entity(e: Entity) {
    call!(0xAD738C3085FE7E11, e, true, true);
}
pub fn delete_entity(e: Entity) {
    let mut handle = e;
    call!(0xAE3CBE5BF394C9C9, &mut handle as *mut i32);
}
pub fn set_entity_heading(e: Entity, heading: f32) {
    call!(0x8E2530AA8ADA980E, e, heading);
}
pub fn set_entity_visible(e: Entity, visible: bool) {
    call!(0xEA1C610A04DB6BBB, e, visible, false);
}
pub fn get_ped_bone_index(ped: Entity, tag: i32) -> i32 {
    ret_i32(call!(0x3F428D08BE5AAE31, ped, tag))
}
pub fn set_entity_has_gravity(e: Entity, toggle: bool) {
    call!(0x4A4722448F18EEF5, e, toggle);
}
pub fn set_ped_gravity(ped: Entity, toggle: bool) {
    call!(0x9FF447B6B6AD960A, ped, toggle);
}
pub fn set_ped_can_ragdoll(ped: Entity, toggle: bool) {
    call!(0xB128377056A54E2A, ped, toggle);
}
pub fn create_object(model: u32, p: GtaVec) -> Entity {
    ret_i32(call!(0x509D5878EB39E842, model, p.x, p.y, p.z, false, false, false))
}
pub fn is_model_in_cdimage(model: u32) -> bool {
    ret_bool(call!(0x35B9E0803292B641, model))
}
pub fn request_model(model: u32) {
    call!(0x963D27A58DF860AC, model);
}
pub fn has_model_loaded(model: u32) -> bool {
    ret_bool(call!(0x98A4EB5D89A0C952, model))
}
pub fn set_model_as_no_longer_needed(model: u32) {
    call!(0xE532F5D78798DAAB, model);
}
pub fn draw_line(a: GtaVec, b: GtaVec, rgba: [u8; 4]) {
    call!(
        0x6B7256074AE34680, a.x, a.y, a.z, b.x, b.y, b.z,
        rgba[0] as i32, rgba[1] as i32, rgba[2] as i32, rgba[3] as i32,
    );
}
/// GTA's own procedural layers (leg/arm/head/torso IK, gestures) write the
/// bones after the animation; the Skate 3 pose replaces all of them.
pub fn set_ped_procedural_layers(ped: Entity, enabled: bool) {
    call!(0x73518ECE2485412B, ped, enabled); // SET_PED_CAN_LEG_IK
    call!(0x6C3B4D6D13B4C841, ped, enabled); // SET_PED_CAN_ARM_IK
    call!(0xC11C18092C5530DC, ped, enabled); // SET_PED_CAN_HEAD_IK
    call!(0xF2B7106D37947CE0, ped, enabled); // SET_PED_CAN_TORSO_IK
    call!(0xBAF20C5432058024, ped, enabled); // SET_PED_CAN_PLAY_GESTURE_ANIMS
    call!(0x6373D1349925A70E, ped, enabled); // SET_PED_CAN_PLAY_AMBIENT_ANIMS
}

/// Screen aspect ratio (width / height).
pub fn get_aspect_ratio() -> f32 {
    ret_f32(call!(0xF1307EF624A80D87, false))
}
pub fn get_frame_time() -> f32 {
    ret_f32(call!(0x15C40837039FFAF7))
}
pub fn disable_control_action(action: i32) {
    call!(0xFE99B66D079CF6BC, 0i32, action, true);
}
pub fn get_disabled_control_normal(action: i32) -> f32 {
    ret_f32(call!(0x11E65974A982637C, 0i32, action))
}
pub fn is_disabled_control_pressed(action: i32) -> bool {
    ret_bool(call!(0xE2587F8CBBD87B1D, 0i32, action))
}
pub fn is_disabled_control_just_pressed(action: i32) -> bool {
    ret_bool(call!(0x91AEF906BCA88877, 0i32, action))
}

pub fn disable_all_control_actions(group: i32) {
    call!(0x5F4B6931816E599B, group);
}
pub fn get_disabled_control_normal_in(group: i32, action: i32) -> f32 {
    ret_f32(call!(0x11E65974A982637C, group, action))
}
pub fn is_disabled_control_just_pressed_in(group: i32, action: i32) -> bool {
    ret_bool(call!(0x91AEF906BCA88877, group, action))
}

pub type Cam = i32;
pub fn create_cam() -> Cam {
    ret_i32(call!(0xC3981DCE61D9E13F, c"DEFAULT_SCRIPTED_CAMERA".as_ptr(), true))
}
pub fn set_cam_active(cam: Cam, active: bool) {
    call!(0x026FB97D0A425F84, cam, active);
}
pub fn render_script_cams(render: bool) {
    call!(0x07E5B515DB0636FC, render, false, 0i32, true, false, 0i32);
}
pub fn destroy_cam(cam: Cam) {
    call!(0x865908C81A2C22E9, cam, false);
}
pub fn set_cam_coord(cam: Cam, p: GtaVec) {
    call!(0x4D41783FB745E42E, cam, p.x, p.y, p.z);
}
pub fn point_cam_at_coord(cam: Cam, p: GtaVec) {
    call!(0xF75497BB865F0803, cam, p.x, p.y, p.z);
}
pub fn set_cam_fov(cam: Cam, fov: f32) {
    call!(0xB13C14F66A00D047, cam, fov);
}

/// Synchronous line-of-sight probe; returns the hit point.
pub fn probe(from: GtaVec, to: GtaVec, flags: i32, ignore: Entity) -> Option<GtaVec> {
    let handle = ret_i32(call!(
        0x377906D8A31E5586, from.x, from.y, from.z, to.x, to.y, to.z, flags, ignore, 7i32,
    ));
    let mut hit: u64 = 0;
    let mut end = NVector3::default();
    let mut normal = NVector3::default();
    let mut entity: u64 = 0;
    let status = ret_i32(call!(
        0x3D87450E15D98694,
        handle,
        &mut hit as *mut u64,
        &mut end as *mut NVector3,
        &mut normal as *mut NVector3,
        &mut entity as *mut u64,
    ));
    (status == 2 && hit as u32 != 0).then(|| GtaVec::new(end.x, end.y, end.z))
}

fn text_component(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

pub fn notify(text: &str) {
    let text = text_component(text);
    call!(0x202709F4C58A0424, c"STRING".as_ptr());
    call!(0x6C188BE134E074AA, text.as_ptr());
    call!(0x2ED7843F8F801023, false, true);
}

pub fn draw_text(text: &str, x: f32, y: f32) {
    let text = text_component(text);
    call!(0x66E0276CC5F6B9DA, 0i32);
    call!(0x07C837F9A01C34C9, 0.0f32, 0.35f32);
    call!(0xBE6B23FFA53FB442, 255i32, 255i32, 255i32, 255i32);
    call!(0x2513DFB0FB8400FE);
    call!(0x25FBB336DF1804CB, c"STRING".as_ptr());
    call!(0x6C188BE134E074AA, text.as_ptr());
    call!(0xCD015E5BB0D96A57, x, y, 0i32);
}
