//! The gameplay pipeline needs more stack than a ScriptHookV script fiber is
//! guaranteed to have, so heavy calls run on a short-lived thread with its
//! own large stack while the fiber waits. GTA natives stay on the fiber.
const STACK_BYTES: usize = 32 << 20;

pub fn run<R: Send>(f: impl FnOnce() -> Result<R, String> + Send) -> Result<R, String> {
    run_serviced(f, || std::thread::sleep(std::time::Duration::from_micros(250)))
}

/// Keep native calls on the calling script fiber while gameplay requests them
/// from its large-stack worker. Never invoke GTA from that worker thread.
pub fn run_serviced<R:Send>(f:impl FnOnce()->Result<R,String>+Send,mut service:impl FnMut())->Result<R,String> {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("skate-gameplay".into())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, f)
            .map_err(|e| format!("cannot start the gameplay thread: {e}"))?;
        while !worker.is_finished(){service();}
        worker.join().unwrap_or_else(|panic| {
            let text = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".into());
            Err(format!("gameplay panicked: {text}"))
        })
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn returns_results_and_turns_panics_into_errors() {
        let mut value = 1;
        assert_eq!(super::run(|| { value += 1; Ok(value) }), Ok(2));
        let error = super::run::<()>(|| panic!("boom")).unwrap_err();
        assert!(error.contains("boom"));
    }

    #[test]
    fn deep_recursion_fits() {
        fn depth(n: u32, pad: [u8; 1024]) -> u32 {
            if n == 0 { pad[0] as u32 } else { depth(n - 1, std::hint::black_box(pad)) + 1 }
        }
        assert_eq!(super::run(|| Ok(depth(8000, [0; 1024]))), Ok(8000));
    }
}
