//! Flat ground shared by the data tests.
use skate_core::{
    math::Vector3,
    physics::{
        board_world::{
            BoardWorld, WorldTriangle,
            query_metadata::{Bounds, QueryMesh, QueryMetadata, QueryPool},
        },
        collision::TriangleFeature,
        contact::RetailContactMaterial,
        drive_frames::RetailAffineTransform,
        world_contact::triangle_from_volume,
    },
};

pub fn flat_world(y: f32) -> BoardWorld {
    let v = Vector3::new;
    let quad = [v(-200., y, -200.), v(200., y, -200.), v(200., y, 200.), v(-200., y, 200.)];
    let material = RetailContactMaterial { static_friction: 0.0, dynamic_friction: 0.0, restitution: 1.0 };
    let triangles: Vec<_> = [[0, 2, 1], [0, 3, 2]]
        .map(|i| WorldTriangle {
            triangle: triangle_from_volume(i.map(|i| quad[i]), 0.0, [1.0; 3], TriangleFeature::ONE_SIDED),
            material,
            tag: 0,
        })
        .to_vec();
    let metadata = QueryMetadata {
        packed_surfaces: vec![0; 2],
        meshes: vec![QueryMesh {
            triangle_range: 0..2,
            local_to_world: RetailAffineTransform::IDENTITY,
            world_to_local: RetailAffineTransform::IDENTITY,
            local_bounds: Bounds::from_points(quad).unwrap(),
            matching_group: -1,
            rejection_flags: 0,
            geometry: 1,
            pool: QueryPool::Ground,
        }],
        static_edges: Vec::new(),
        island_flags: 0,
    };
    BoardWorld::with_query_metadata(triangles, metadata).unwrap()
}

