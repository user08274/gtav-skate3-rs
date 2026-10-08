//! One physical solve for the board, skater and original animation targets.
//! The caller publishes forces and drive targets before entering this phase.
mod assembly_contacts;
mod diagnostics;
use super::{GamePhysics, SkaterRuntime, colliders, skeleton_colliders};
use skate_core::physics::{
    board::BodyId,
    board_step::{ATTACHED_REACTION_BASE, AttachedStep},
    skeleton_animation_record::{AnimationPartTransform, IDENTITY},
    skeleton_body::PART_COUNT,
};

pub(super) fn advance(physics:&mut GamePhysics, skater:&mut SkaterRuntime,truck_targets:[f32;2])->Result<(),String> {
    advance_inner(physics,skater,truck_targets)
}
fn advance_inner(
    physics: &mut GamePhysics,
    skater: &mut SkaterRuntime,
    truck_targets: [f32; 2],
) -> Result<(), String> {
    let before = diagnostics::snapshot(physics, skater);
    diagnostics::validate(&before, "before shared solve").map_err(|error| format!(
        "{error}; com_frame={:?}; lifted_com_frame={:?}; animation_root={:?}; biped_position={:?}; biped_surface={:?}",
        skater.animated_skeleton.board_frames.com_frame,
        skater.animated_skeleton.board_frames.lifted_com_frame,
        skater.animated_skeleton.roots.animation_to_world,
        skater.biped_ground.controller.state.position_368,
        skater.biped_ground.controller.state.surface,
    ))?;
    let mut board_volumes = colliders::world_volumes(&physics.board, &physics.settings);
    board_volumes.retain(|volume| skater.board_possession_live.volume_enabled(volume.body));
    let skeleton_volumes =
        skeleton_colliders::enabled_volumes(&skater.skeleton, &skater.skeleton_collision)?;
    // Each native assembly has its own query record and retention buffer.
    // Skeleton82BE5094 passes false to82768728: its edge threshold is -1,
    // whereas the board requests .999. GroundPipeline supplies the remaining
    // shared values. Do not let the second query overwrite the first's rows.
    let mut contacts = Vec::new();
    let mut skeleton_query = physics.query;
    skeleton_query.edge_cos_bend_normal_threshold = -1.0;
    let mut skeleton_world_volumes = skeleton_volumes.clone();
    skeleton_colliders::retain_world_volumes(&mut skeleton_world_volumes, &skater.skeleton_collision);
    if !physics.native_contacts {
        contacts.extend_from_slice(physics.world.query_primitives(
            &board_volumes, physics.query, physics.retention,
        ));
    }else{
        // Retain only ray-confirmed narrow tubes as a stable contact mesh.
        // The sampled ground and former obstacle boxes stay excluded.
        contacts.extend_from_slice(physics.native_rails.query_primitives(&board_volumes,physics.query,physics.retention));
        contacts.extend_from_slice(physics.native_rails.query_primitives(&skeleton_world_volumes,skeleton_query,physics.retention));
        contacts.extend_from_slice(physics.world.query_primitives(
            &skeleton_world_volumes, skeleton_query, physics.retention,
        ));
    }
    // Live host faces at actual shape-test hits, instead of obstacle boxes.
    // Native geometry replaces the terrain approximation instead of adding
    // competing constraints on curved surfaces.
    for (volumes, query) in [(&board_volumes,physics.query),(&skeleton_world_volumes,skeleton_query)] {
        for volume in volumes {
            let (center,radius)=super::network::bounds(volume.primitive);
            let center=skate_core::math::Vector3::new(center.x,center.y,center.z);
            let triangles:Vec<_>=physics.world.external_nearby(center,radius+0.02).into_iter()
                .filter_map(|vertices|skate_core::physics::board_world::WorldTriangle::from_vertices(vertices,
                    skate_core::physics::contact::RetailContactMaterial{static_friction:0.,dynamic_friction:0.,restitution:1.},
                    // A hit patch has no physical rim or corners. Suppress
                    // artificial edge/vertex reactions between tangent faces.
                    0,0xf10,[1.;3],0.))
                .filter(|t| {
                    let v=t.triangle.vertices;let n=t.triangle.feature.normal;
                    let c=skate_core::math::Vector3::new((v[0].x+v[1].x+v[2].x)/3.,(v[0].y+v[1].y+v[2].y)/3.,(v[0].z+v[1].z+v[2].z)/3.);
                    let a=skate_core::math::Vector3::new(c.x+n.x*0.01,c.y+n.y*0.01,c.z+n.z*0.01);
                    let d=skate_core::math::Vector3::new(-n.x*0.02,-n.y*0.02,-n.z*0.02);
                    !physics.world.triangles().iter().any(|existing|{
                        if physics.native_contacts && existing.tag!=crate::host::HOST_RAIL_TAG{return false;}
                        let mut hit=skate_core::physics::triangle_query::TriangleLineHit{position:c,normal:n,fraction:0.,volume_parameter:[0.;3]};
                        skate_core::physics::triangle_query::triangle_segment(&mut hit,a,d,existing.triangle.vertices,0.,0.)
                    })
                }).collect();
            if !triangles.is_empty(){
                let mut local=skate_core::physics::board_world::BoardWorld::new(triangles);
                contacts.extend_from_slice(local.query_primitives(std::slice::from_ref(volume),query,physics.retention));
            }
        }
    }
    assembly_contacts::append(
        &mut contacts,
        &board_volumes,
        &skeleton_volumes,
        physics.board.collision_group(),
        &skater.skeleton_collision,
    )?;
    // Remote actors use separate reaction indices after skeleton and targets.
    // Never apply the single-skater self-culling bitmap to another player.
    let before_remote = contacts.len();
    for a in board_volumes.iter().chain(&skeleton_volumes) {
        for b in &physics.network_proxies.volumes {
            let (ac, ar) = super::network::bounds(a.primitive);
            let (bc, br) = super::network::bounds(b.primitive);
            if ac.distance_squared(bc) <= (ar + br + 0.05).powi(2) {
                assembly_contacts::append_pair(&mut contacts, a, b);
            }
        }
    }
    super::solid_contacts::append(&mut contacts, &board_volumes,
        &physics.network_proxies.solids, physics, skater);
    super::solid_contacts::append(&mut contacts, &skeleton_volumes,
        &physics.network_proxies.solids, physics, skater);
    physics.network_contacts = contacts.len() - before_remote;
    physics.contact_count = contacts.len();
    let dt = physics.settings.step.simulation.time_step;
    let mut joints = skate_core::physics::skeleton_body::SkeletonJoints {
        records: skater.skeleton_joints.records,
    }
    .build(skater.skeleton.bodies(), ATTACHED_REACTION_BASE, dt);
    let mut drives = skater.skeleton_drives.build(
        skater.skeleton.bodies(),
        ATTACHED_REACTION_BASE,
        ATTACHED_REACTION_BASE + PART_COUNT,
        dt,
    );
    let skeleton_drive_count = drives.rows.len();
    //82D74FD8: persistent hand drives share the deck and skeleton reactions.
    skater.board_possession.append_drives(
        physics.board.bodies()[BodyId::Deck.index()],
        [skater.skeleton.bodies()[3], skater.skeleton.bodies()[7]],
        BodyId::Deck.index(),
        [ATTACHED_REACTION_BASE + 3, ATTACHED_REACTION_BASE + 7],
        dt,
        &mut drives.rows,
    );
    let bodies = skater
        .skeleton
        .bodies_mut()
        .iter_mut()
        .chain(skater.skeleton_drives.targets.bodies.iter_mut())
        .chain(physics.network_proxies.bodies.iter_mut())
        .collect();
    physics.board.advance_attached(
        &contacts,
        truck_targets,
        physics.settings.step,
        AttachedStep {
            bodies,
            contacts: &mut [],
            joints: &mut joints,
            drives: &mut drives.rows,
        },
    );
    physics.network_proxies.capture_dynamics_reactions(physics.board.solved_reactions(), dt);
    if let Err(error) = diagnostics::validate(
        &diagnostics::snapshot(physics, skater), "after shared solve",
    ) {
        return Err(format!("{error}; input_bodies={before:?}; contacts={contacts:?}; joints={joints:?}; drives={:?}", drives.rows));
    }
    skater
        .skeleton
        .publish_physical_record(deck_frame(&physics.board));
    // These solved rows are consumed by the actual collision/drive feedback
    // phase; keep their identity and impulses after the shared solve.
    // Possession drives have no skeleton spy identity. They participate in the
    // same solve above, but must not enter the skeleton-only feedback batch.
    drives.rows.truncate(skeleton_drive_count);
    skater.solved_drives = Some(drives);
    Ok(())
}

pub(super) fn deck_frame(
    board: &skate_core::physics::board_runtime::BoardRuntime,
) -> AnimationPartTransform {
    let deck = board.part_transforms()[BodyId::Deck.index()];
    let mut frame = IDENTITY;
    for (axis, column) in deck.basis.columns.iter().enumerate() {
        frame[axis][..3].copy_from_slice(column);
    }
    frame[3] = [
        deck.translation.x,
        deck.translation.y,
        deck.translation.z,
        0.0,
    ];
    frame
}
