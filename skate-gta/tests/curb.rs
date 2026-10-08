//! Requires the player's converted Skate 3 files (SKATE_GTA_ASSETS).
use skate_gameplay::host::Mode;
use skate_gta::{coords::GtaVec, ride::Ride, terrain::{GroundProbe, PatchSettings}};
use std::time::Duration;

const STREET: f32 = 31.5;
const CURB_Y: f32 = 206.13;
struct Curb { bevel: bool, incline: bool }
impl GroundProbe for Curb {
    fn down(&mut self, _x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
        let rise = if self.bevel { ((y - CURB_Y) / 0.05).clamp(0., 1.) * 0.15 }
            else if y >= CURB_Y { 0.15 } else { 0. };
        let z = STREET + rise + if self.incline { (y - 200.) * 0.02 } else { 0. };
        (top >= z && bottom <= z).then_some(z)
    }
    fn toward(&mut self,a:GtaVec,b:GtaVec)->Option<(GtaVec,GtaVec)> {
        let d=b.sub(a);let incline=if self.incline{0.02}else{0.};
        let mut hits=Vec::new();
        let mut planes=vec![(f32::NEG_INFINITY,CURB_Y,0.,0.)];
        if self.bevel {planes.push((CURB_Y,CURB_Y+0.05,0.,3.));}
        planes.push((CURB_Y+if self.bevel{0.05}else{0.},f32::INFINITY,0.15,0.));
        for (low,high,rise,bevel_slope) in planes {
            let slope=incline+bevel_slope;
            let z=STREET+incline*(a.y-200.)+rise+bevel_slope*(a.y-CURB_Y);
            let denominator=d.z-slope*d.y;
            if denominator.abs()<1e-7 {continue;}
            let t=(z-a.z)/denominator;let p=a.add(d.scale(t));
            if (0. ..=1.).contains(&t) && p.y>=low && p.y<=high {
                hits.push((t,p,GtaVec::new(0.,-slope,1.).normalized()));
            }
        }
        if !self.bevel && d.y.abs()>1e-7 {
            let t=(CURB_Y-a.y)/d.y;let p=a.add(d.scale(t));
            let bottom=STREET+incline*(CURB_Y-200.);
            if (0. ..=1.).contains(&t) && (bottom..=bottom+0.15).contains(&p.z){
                hits.push((t,p,GtaVec::new(0.,-1.,0.)));
            }
        }
        hits.into_iter().min_by(|a,b|a.0.total_cmp(&b.0)).map(|(_,p,n)|(p,n))
    }
}

fn ride_at_curb(ollie_at: Option<f32>, bevel: bool, incline: bool) -> (f32, f32) {
    ride_at_curb_mode(ollie_at,bevel,incline,false)
}
fn ride_at_curb_mode(ollie_at:Option<f32>,bevel:bool,incline:bool,live:bool)->(f32,f32){
    let root = std::env::var_os("SKATE_GTA_ASSETS").unwrap();
    let mut probe = Curb { bevel, incline };
    let mut ride = Ride::start(std::path::Path::new(&root), Mode::Easy,
        GtaVec::new(100., 200., STREET), 0., PatchSettings::default(), &mut probe).unwrap();
    ride.native_collision=live;ride.find_walls=!live;
    let (mut furthest, mut top) = (0.0f32, f32::MIN);
    let mut ollie = None;
    for i in 0..480u32 {
        let mut pad = [0.; 18];
        pad[16] = if i < 240 && i % 60 < 30 && ollie.is_none() { 1. } else { 0. };
        if ollie.is_none() && ollie_at.is_some_and(|d| CURB_Y - ride.deck_position().y < d) {
            ollie = Some(i);
        }
        if let Some(start) = ollie {
            pad[4] = match i - start { 0..10 => -1., 10..14 => 1., _ => 0. };
        }
        if live {
            ride.refresh_world(&mut probe).unwrap();
            ride.advance_native(Duration::from_secs_f32(1./60.),4,pad,&mut probe).unwrap();
        }else{ride.advance(Duration::from_secs_f32(1. / 60.), 4, pad, &mut probe).unwrap();}
        let deck = ride.deck_position();
        furthest = furthest.max(deck.y);
        if deck.y > CURB_Y + 0.5 { top = top.max(deck.z); }
    }
    (furthest, top)
}

#[test]
fn native_contacts_stop_at_a_curb_and_clear_it_with_an_ollie(){
    if std::env::var_os("SKATE_GTA_ASSETS").is_none(){return;}
    on_big_stack(||{
        let (rolling,_)=ride_at_curb_mode(None,false,false,true);
        let (jumping,top)=ride_at_curb_mode(Some(2.5),false,false,true);
        eprintln!("native curb rolling={rolling} jumping={jumping} top={top}");
        assert!(rolling<CURB_Y+0.5,"native board rolled up curb");
        assert!(jumping>CURB_Y+1. && top>STREET+0.15,"native ollie failed to clear curb");
    });
}

fn on_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(1 << 20).spawn(f).unwrap().join().unwrap();
}

#[test]
fn the_board_does_not_roll_up_a_curb() {
    if std::env::var_os("SKATE_GTA_ASSETS").is_none() { return; }
    on_big_stack(|| {
        let (furthest, _) = ride_at_curb(None, false, false);
        assert!(furthest < CURB_Y + 0.5, "the board rolled up the curb ({furthest})");
    });
}

#[test]
fn an_ollie_gets_onto_the_curb() {
    if std::env::var_os("SKATE_GTA_ASSETS").is_none() { return; }
    on_big_stack(|| {
        // The stock graph delays takeoff after the stick gesture. Start before
        // the front wheels reach the vertical face, rather than its old ramp.
        let (furthest, top) = ride_at_curb(Some(2.5), false, false);
        assert!(furthest > CURB_Y + 1., "the ollie did not get over the curb ({furthest})");
        assert!(top > STREET + 0.15, "the board did not get above the curb ({top})");
    });
}

#[test]
fn bevelled_and_inclined_curbs_block_rolling() {
    if std::env::var_os("SKATE_GTA_ASSETS").is_none() { return; }
    on_big_stack(|| {
        for (bevel, incline) in [(true, false), (false, true), (true, true)] {
            let (furthest, _) = ride_at_curb(None, bevel, incline);
            assert!(furthest < CURB_Y + 0.5,
                "rolled over curb: bevel={bevel} incline={incline} y={furthest}");
        }
    });
}
