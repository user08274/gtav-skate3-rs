//! Types the gameplay modules borrowed from the upstream skate-mods crate.
use serde::Deserialize;

/// Replacement values for one stock camera shot (`camera_shots` collection), named
/// after the retail attributes and in their units (metres, degrees, seconds). Unset
/// fields keep the stock value. Applies to every place the shot is used, including
/// blend trees that name it as a child.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CameraShotTuning {
    #[serde(rename = "PositionDistance")]
    pub position_distance: Option<f32>,
    #[serde(rename = "PositionElevation")]
    pub position_elevation: Option<f32>,
    #[serde(rename = "PositionHeading")]
    pub position_heading: Option<f32>,
    #[serde(rename = "FramingLensLength")]
    pub framing_lens_length: Option<f32>,
    #[serde(rename = "FramingRoll")]
    pub framing_roll: Option<f32>,
    #[serde(rename = "FramingYaw")]
    pub framing_yaw: Option<f32>,
    #[serde(rename = "FramingPitch")]
    pub framing_pitch: Option<f32>,
    #[serde(rename = "ReferenceBoardOffset")]
    pub reference_board_offset: Option<f32>,
    #[serde(rename = "SmoothingDirection")]
    pub smoothing_direction: Option<f32>,
    #[serde(rename = "SmoothingElevation")]
    pub smoothing_elevation: Option<f32>,
    #[serde(rename = "SmoothingYaw")]
    pub smoothing_yaw: Option<f32>,
    #[serde(rename = "SmoothingPitch")]
    pub smoothing_pitch: Option<f32>,
    #[serde(rename = "TransitionTime")]
    pub transition_time: Option<f32>,
}
