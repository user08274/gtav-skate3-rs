//! The Skate 3 Rust Engine gameplay modules only use Bevy for logging, glam
//! math and a plain `Transform`. This crate provides exactly those, so the
//! modules build without the engine.
pub use glam;

pub mod math {
    pub use glam::*;
}

pub mod log {
    use std::sync::RwLock;

    pub type Sink = fn(Level, &str);
    static SINK: RwLock<Option<Sink>> = RwLock::new(None);

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Level {
        Info,
        Warn,
        Error,
    }

    pub fn set_sink(sink: Option<Sink>) {
        if let Ok(mut s) = SINK.write() {
            *s = sink;
        }
    }

    #[doc(hidden)]
    pub fn emit(level: Level, message: std::fmt::Arguments<'_>) {
        if let Ok(s) = SINK.read() {
            if let Some(sink) = *s {
                sink(level, &message.to_string());
            }
        }
    }

    /// Accepts both format strings and tracing-style `field = value, "msg"`
    /// records; the latter are logged as their source text.
    #[macro_export]
    #[doc(hidden)]
    macro_rules! __shim_log {
        ($level:expr, target: $target:expr, $($rest:tt)*) => { $crate::__shim_log!($level, $($rest)*) };
        ($level:expr, $fmt:literal $(, $($arg:tt)*)?) => {
            $crate::log::emit($level, format_args!($fmt $(, $($arg)*)?))
        };
        ($level:expr, $($arg:tt)*) => {
            $crate::log::emit($level, format_args!("{}", stringify!($($arg)*)))
        };
    }
    #[macro_export]
    macro_rules! info {
        ($($arg:tt)*) => { $crate::__shim_log!($crate::log::Level::Info, $($arg)*) };
    }
    #[macro_export]
    macro_rules! debug {
        ($($arg:tt)*) => { $crate::__shim_log!($crate::log::Level::Info, $($arg)*) };
    }
    #[macro_export]
    macro_rules! warn {
        ($($arg:tt)*) => { $crate::__shim_log!($crate::log::Level::Warn, $($arg)*) };
    }
    #[macro_export]
    macro_rules! error {
        ($($arg:tt)*) => { $crate::__shim_log!($crate::log::Level::Error, $($arg)*) };
    }
    #[macro_export]
    macro_rules! info_span {
        ($($arg:tt)*) => { $crate::log::Span };
    }
    pub use crate::{debug, error, info, info_span, warn};

    pub struct Span;
    pub struct Entered;
    impl Span {
        pub fn in_scope<T>(&self, f: impl FnOnce() -> T) -> T {
            f()
        }
        pub fn entered(self) -> Entered {
            Entered
        }
    }
}

pub mod transform {
    use glam::{Mat4, Quat, Vec3};

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Transform {
        pub translation: Vec3,
        pub rotation: Quat,
        pub scale: Vec3,
    }

    impl Default for Transform {
        fn default() -> Self {
            Self::IDENTITY
        }
    }

    impl Transform {
        pub const IDENTITY: Self = Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        pub fn from_matrix(matrix: Mat4) -> Self {
            let (scale, rotation, translation) = matrix.to_scale_rotation_translation();
            Self { translation, rotation, scale }
        }
        pub fn from_translation(translation: Vec3) -> Self {
            Self { translation, ..Self::IDENTITY }
        }
        pub fn from_rotation(rotation: Quat) -> Self {
            Self { rotation, ..Self::IDENTITY }
        }
        pub fn with_rotation(mut self, rotation: Quat) -> Self {
            self.rotation = rotation;
            self
        }
        pub fn with_translation(mut self, translation: Vec3) -> Self {
            self.translation = translation;
            self
        }
        pub fn to_matrix(&self) -> Mat4 {
            Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
        }
        pub fn compute_matrix(&self) -> Mat4 {
            self.to_matrix()
        }
        /// Bevy's forward is local -Z.
        pub fn forward(&self) -> Vec3 {
            self.rotation * Vec3::NEG_Z
        }
    }
}

pub mod prelude {
    pub use crate::log::{debug, error, info, info_span, warn};
    pub fn default<T: Default>() -> T {
        T::default()
    }
    pub use crate::transform::Transform;
    pub use glam::{EulerRot, Mat3, Mat4, Quat, Vec2, Vec3, Vec4};
}
