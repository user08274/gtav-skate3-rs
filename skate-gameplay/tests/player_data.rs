//! Full gameplay pipeline on flat ground with the player's converted files.
//! Set SKATE_GTA_ASSETS to a skate3rust `assets` folder; skipped otherwise.
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
    player::state::PhysicalStateId,
};
use skate_gameplay::host::{Game, Mode, SPAWN_GROUND_HEIGHT};
use std::path::PathBuf;

const A: usize = 16;
const LEFT_STICK_X: usize = 0;

fn root() -> Option<PathBuf> {
    std::env::var_os("SKATE_GTA_ASSETS").map(PathBuf::from)
}

fn flat(y: f32) -> BoardWorld {
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

fn game(root: &PathBuf) -> Game {
    let mut game = Game::load(root, Mode::Easy).expect("gameplay loads from the player's files");
    game.set_world(flat(SPAWN_GROUND_HEIGHT));
    game
}

fn run(game: &mut Game, ticks: u32, pad: impl Fn(u32) -> [f32; 18]) {
    for i in 0..ticks {
        game.tick(pad(i)).unwrap_or_else(|e| panic!("tick {}: {e}", game.ticks()));
    }
}

#[test]
fn skater_stands_on_the_board_and_stays_on_the_ground() {
    let Some(root) = root() else { return };
    let mut game = game(&root);
    run(&mut game, 180, |_| [0.0; 18]);
    assert_eq!(game.state(), PhysicalStateId::PhysicsGround);
    assert!(game.wheel_contacts() > 0);
    let hips = game.skater_body_positions()[0];
    assert!(hips.y > 0.5, "skater stands on the deck: hips at {hips:?}");
    assert!(game.board_speed() < 0.2, "resting speed {}", game.board_speed());
}

#[test]
fn pushing_with_a_accelerates_and_the_stick_turns() {
    let Some(root) = root() else { return };
    let mut game = game(&root);
    run(&mut game, 60, |_| [0.0; 18]);
    let start = game.deck().translation;
    // Tap A like a player pushing: half a second held, half released.
    run(&mut game, 240, |i| {
        let mut pad = [0.0; 18];
        pad[A] = if i % 60 < 30 { 1.0 } else { 0.0 };
        pad
    });
    let speed = game.board_speed();
    let end = game.deck().translation;
    let travelled = ((end.x - start.x).powi(2) + (end.z - start.z).powi(2)).sqrt();
    eprintln!("after pushing: speed {speed:.2} m/s, travelled {travelled:.2} m, state {:?}", game.state());
    assert!(speed > 1.0 && travelled > 2.0);
    let heading = |g: &Game| {
        let at = g.deck().basis.columns[2];
        at[0].atan2(at[2])
    };
    let before = heading(&game);
    run(&mut game, 90, |_| {
        let mut pad = [0.0; 18];
        pad[LEFT_STICK_X] = 1.0;
        pad
    });
    let turned = (heading(&game) - before).abs();
    eprintln!("turned {:.1} degrees, state {:?}", turned.to_degrees(), game.state());
    assert!(turned > 0.2, "the left stick steers");
}
