//! ScriptHookV entry points, resolved at runtime from the already loaded
//! ScriptHookV.dll (MSVC-mangled exports), so no import library is needed.
use std::sync::OnceLock;
use windows_sys::Win32::{
    Foundation::HMODULE,
    System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
};

pub type ScriptMain = unsafe extern "C" fn();

pub struct Api {
    script_register: unsafe extern "C" fn(HMODULE, ScriptMain),
    script_unregister: unsafe extern "C" fn(HMODULE),
    script_wait: unsafe extern "C" fn(u32),
    native_init: unsafe extern "C" fn(u64),
    native_push64: unsafe extern "C" fn(u64),
    native_call: unsafe extern "C" fn() -> *mut u64,
    handle_address: unsafe extern "C" fn(i32) -> *mut u8,
    create_texture: unsafe extern "C" fn(*const u8) -> i32,
    #[allow(clippy::type_complexity)]
    draw_texture: unsafe extern "C" fn(i32, i32, i32, i32, f32, f32, f32, f32, f32, f32, f32, f32, f32, f32, f32, f32),
}

// Function pointers into ScriptHookV; they are valid for the process lifetime.
unsafe impl Send for Api {}
unsafe impl Sync for Api {}

static API: OnceLock<Api> = OnceLock::new();

macro_rules! resolve {
    ($module:expr, $name:literal) => {{
        let address = unsafe { GetProcAddress($module, concat!($name, "\0").as_ptr()) };
        match address {
            Some(f) => unsafe { std::mem::transmute(f) },
            None => return Err(concat!("ScriptHookV export missing: ", $name)),
        }
    }};
}

pub fn load() -> Result<&'static Api, &'static str> {
    if let Some(api) = API.get() {
        return Ok(api);
    }
    let module = unsafe { GetModuleHandleA(c"ScriptHookV.dll".as_ptr().cast()) };
    if module.is_null() {
        return Err("ScriptHookV.dll is not loaded");
    }
    let api = Api {
        script_register: resolve!(module, "?scriptRegister@@YAXPEAUHINSTANCE__@@P6AXXZ@Z"),
        script_unregister: resolve!(module, "?scriptUnregister@@YAXPEAUHINSTANCE__@@@Z"),
        script_wait: resolve!(module, "?scriptWait@@YAXK@Z"),
        native_init: resolve!(module, "?nativeInit@@YAX_K@Z"),
        native_push64: resolve!(module, "?nativePush64@@YAX_K@Z"),
        native_call: resolve!(module, "?nativeCall@@YAPEA_KXZ"),
        handle_address: resolve!(module, "?getScriptHandleBaseAddress@@YAPEAEH@Z"),
        create_texture: resolve!(module, "?createTexture@@YAHPEBD@Z"),
        draw_texture: resolve!(module, "?drawTexture@@YAXHHHHMMMMMMMMMMMM@Z"),
    };
    Ok(API.get_or_init(|| api))
}

fn api() -> &'static Api {
    API.get().expect("ScriptHookV API is loaded before scripts run")
}

pub fn register(module: HMODULE, main: ScriptMain) -> Result<(), &'static str> {
    let api = load()?;
    unsafe { (api.script_register)(module, main) };
    Ok(())
}

pub fn unregister(module: HMODULE) {
    if let Some(api) = API.get() {
        unsafe { (api.script_unregister)(module) };
    }
}

/// Yields the script fiber; 0 resumes on the next game frame.
pub fn wait(ms: u32) {
    unsafe { (api().script_wait)(ms) };
}

/// Game object behind a script handle (CEntity* for entities), or null.
pub fn handle_address(handle: i32) -> *mut u8 {
    unsafe { (api().handle_address)(handle) }
}

/// Loads an image file (PNG) as a screen texture; returns its id. Textures
/// live until scripts are reloaded. Must run on a script fiber.
pub fn create_texture(path: &std::path::Path) -> Result<i32, String> {
    let text = path.to_str().ok_or("texture path is not valid text")?;
    // ScriptHookV takes an ANSI path; keep to what survives that.
    if !text.is_ascii() {
        return Err(format!("texture path must be plain ASCII: {text}"));
    }
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    Ok(unsafe { (api().create_texture)(bytes.as_ptr()) })
}

/// One on-screen instance of a texture (see ScriptHookV's main.h).
pub struct TextureDraw {
    pub id: i32,
    pub instance: i32,
    pub level: i32,
    pub time_ms: i32,
    pub size: [f32; 2],
    pub center: [f32; 2],
    pub position: [f32; 2],
    pub rotation: f32,
    pub aspect: f32,
    pub color: [f32; 4],
}

pub fn draw_texture(d: &TextureDraw) {
    unsafe {
        (api().draw_texture)(
            d.id, d.instance, d.level, d.time_ms, d.size[0], d.size[1], d.center[0], d.center[1],
            d.position[0], d.position[1], d.rotation, d.aspect, d.color[0], d.color[1], d.color[2], d.color[3],
        )
    }
}

/// Invoke a native by hash with 64-bit argument words.
///
/// # Safety
/// The hash, argument count and argument kinds must match the native, and any
/// pointer arguments must stay valid for the call. Must run on a script fiber.
pub unsafe fn invoke(hash: u64, args: &[u64]) -> *const u64 {
    let api = api();
    unsafe {
        (api.native_init)(hash);
        for &arg in args {
            (api.native_push64)(arg);
        }
        (api.native_call)()
    }
}
