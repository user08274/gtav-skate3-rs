//! Skate 3 gameplay camera simulation. The upstream Bevy rendering endpoint is
//! not ported; hosts read `CameraRuntime::frame` and drive their own camera.
mod settings;
mod collision;
mod world_query;
mod shot_data;
mod stock_names;
mod shake_data;
mod trajectory;
mod subject;
mod graph_subject;
mod graph_conditions;
mod graph;
mod runtime;
mod publication;
pub mod angle;
pub(crate) use publication::{
    snapshot as publish_camera_subject, CameraPublicationInputs, CameraStateOutput,
    CameraAnimationOutput, CameraAirOutput, CameraOffboardOutput, CameraGrindOutput,
    CameraEventsOutput, CameraPreferences,
};
pub(crate) use graph_subject::CameraGraphEnvironment;
pub(crate) use runtime::CameraRuntime;
pub(crate) use angle::CameraAngle;
