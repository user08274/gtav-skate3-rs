//! The Skate 3 runtime's 48 kHz stereo to the Windows sound device (waveOut), on a
//! thread of its own that pulls `Runtime::fill_stereo` block by block. One device for
//! the whole game; a ride hands it its runtime ([`set_runtime`]) and takes it back at
//! the end. Silent while nothing rides or the game is paused.
use skate_gameplay::game_audio::Shared;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Media::Audio::{
    CALLBACK_EVENT, HWAVEOUT, WAVE_FORMAT_PCM, WAVE_MAPPER, WAVEFORMATEX, WAVEHDR, WHDR_DONE, waveOutClose, waveOutOpen,
    waveOutPrepareHeader, waveOutWrite,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

const RATE: u32 = 48_000;
/// Buffers of 512 frames (10.7 ms): 6 queued = 64 ms of latency.
const FRAMES: usize = 512;
const BUFFERS: usize = 6;

static RUNTIME: Mutex<Option<Shared>> = Mutex::new(None);
static VOLUME: AtomicU32 = AtomicU32::new(0x3F80_0000); // 1.0
static PAUSED: AtomicBool = AtomicBool::new(false);
static STARTED: OnceLock<Result<(), String>> = OnceLock::new();

/// The runtime to play (None: silence). Starts the device on first use.
pub fn set_runtime(runtime: Option<Shared>) -> Result<(), String> {
    *RUNTIME.lock().unwrap_or_else(|e| e.into_inner()) = runtime;
    STARTED.get_or_init(start).clone()
}

pub fn set_volume(volume: f32) {
    VOLUME.store(volume.max(0.0).to_bits(), Ordering::Relaxed);
}

/// The game is paused (pause menu): the runtime is not pulled, so it resumes where it was.
pub fn set_paused(paused: bool) {
    PAUSED.store(paused, Ordering::Relaxed);
}

struct Device {
    handle: HWAVEOUT,
    event: HANDLE,
}
// The handles are used by the output thread alone after the open.
unsafe impl Send for Device {}

fn start() -> Result<(), String> {
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 2,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * 4,
        nBlockAlign: 4,
        wBitsPerSample: 16,
        cbSize: 0,
    };
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
    if event.is_null() {
        return Err("cannot create the sound event".into());
    }
    let mut handle: HWAVEOUT = std::ptr::null_mut();
    let result = unsafe { waveOutOpen(&mut handle, WAVE_MAPPER, &format, event as usize, 0, CALLBACK_EVENT) };
    if result != 0 {
        unsafe { CloseHandle(event) };
        return Err(format!("no sound device (waveOutOpen error {result})"));
    }
    let device = Device { handle, event };
    std::thread::Builder::new()
        .name("skate3-audio".into())
        .stack_size(1 << 20)
        .spawn(move || pump(device))
        .map_err(|e| format!("cannot start the sound thread: {e}"))?;
    Ok(())
}

fn fill(out: &mut [i16], mix: &mut [f32]) {
    let runtime = RUNTIME.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let playing = runtime.filter(|_| !PAUSED.load(Ordering::Relaxed));
    match playing {
        Some(runtime) => {
            match runtime.lock() {
                Ok(mut rt) => rt.fill_stereo(mix),
                Err(_) => mix.fill(0.0),
            }
            let volume = f32::from_bits(VOLUME.load(Ordering::Relaxed));
            for (o, v) in out.iter_mut().zip(mix.iter()) {
                *o = ((v * volume).clamp(-1.0, 1.0) * 32767.0) as i16;
            }
        }
        None => out.fill(0),
    }
}

fn pump(device: Device) {
    let mut data: Vec<Vec<i16>> = (0..BUFFERS).map(|_| vec![0i16; FRAMES * 2]).collect();
    let mut mix = vec![0.0f32; FRAMES * 2];
    let size = std::mem::size_of::<WAVEHDR>() as u32;
    let mut headers: Vec<WAVEHDR> = data
        .iter_mut()
        .map(|d| WAVEHDR {
            lpData: d.as_mut_ptr() as *mut u8,
            dwBufferLength: (FRAMES * 4) as u32,
            dwBytesRecorded: 0,
            dwUser: 0,
            dwFlags: 0,
            dwLoops: 0,
            lpNext: std::ptr::null_mut(),
            reserved: 0,
        })
        .collect();
    for (h, d) in headers.iter_mut().zip(data.iter_mut()) {
        fill(d, &mut mix);
        unsafe {
            waveOutPrepareHeader(device.handle, h, size);
            waveOutWrite(device.handle, h, size);
        }
    }
    loop {
        unsafe { WaitForSingleObject(device.event, 50) };
        for (h, d) in headers.iter_mut().zip(data.iter_mut()) {
            // The driver sets WHDR_DONE when it has played a buffer.
            let flags = unsafe { std::ptr::read_volatile(std::ptr::addr_of!(h.dwFlags)) };
            if flags & WHDR_DONE != 0 {
                fill(d, &mut mix);
                h.dwFlags &= !WHDR_DONE;
                unsafe { waveOutWrite(device.handle, h, size) };
            }
        }
    }
    #[allow(unreachable_code)]
    unsafe {
        waveOutClose(device.handle);
        CloseHandle(device.event);
    }
}
