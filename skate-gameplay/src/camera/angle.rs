//! Retail "Camera Angle" setting (High / Low), the stock camera graph's
//! `IsCameraTypeActive` value. Persistence and the menu are not ported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraAngle {
    Low,
    #[default]
    High,
}

impl CameraAngle {
    pub fn graph_type(self) -> u32 {
        match self {
            Self::Low => 0,
            Self::High => 1,
        }
    }
}
