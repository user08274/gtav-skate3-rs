//! End-to-end native gameplay on a rail too thin for the height-field grid.
use skate_gameplay::host::Mode;
use skate_gta::{coords::GtaVec,ride::Ride,terrain::{GroundProbe,PatchSettings}};
use std::time::Duration;
struct Tube { panel: bool }
impl GroundProbe for Tube {
    fn down(&mut self,x:f32,y:f32,top:f32,bottom:f32)->Option<f32>{
        let z=if (100.55..=100.61).contains(&x) && (190. ..=300.).contains(&y){31.95}else{31.5};
        (top>=z && bottom<=z).then_some(z)
    }
    fn toward(&mut self,from:GtaVec,to:GtaVec)->Option<(GtaVec,GtaVec)>{
        let d=to.sub(from);
        let mut hits=Vec::new();
        if d.x!=0. {for x in [100.55,100.61] {
            let t=(x-from.x)/d.x;let p=from.add(d.scale(t));
            let bottom=if self.panel{31.5}else{31.89};
            if (0. ..=1.).contains(&t) && (190. ..=300.).contains(&p.y) && (bottom..=31.95).contains(&p.z){
                hits.push((t,p,GtaVec::new(if x<100.6{-1.}else{1.},0.,0.)));
            }
        }}
        if d.z!=0. {for z in [31.5,31.89,31.95] {
            if self.panel && z==31.89 {continue;}
            let t=(z-from.z)/d.z;let p=from.add(d.scale(t));
            if (0. ..=1.).contains(&t) && (z==31.5 || ((100.55..=100.61).contains(&p.x) && (190. ..=300.).contains(&p.y))){
                hits.push((t,p,GtaVec::new(0.,0.,if z==31.89{-1.}else{1.})));
            }
        }}
        hits.into_iter().min_by(|a,b|a.0.total_cmp(&b.0)).map(|(_,p,n)|(p,n))
    }
}
#[test]
fn a_thin_rail_is_detected_and_can_be_ground() {
    check_rail(false,false,false);
}
#[test]
fn actual_shape_queries_support_grinding_without_obstacle_boxes(){
    check_rail(true,false,false);
}
#[test]
fn a_fence_panel_top_supports_native_grinding(){check_rail(true,true,false);}
#[test]
fn extending_the_provider_during_a_native_grind_preserves_the_grind(){check_rail(true,false,true);}
fn check_rail(live:bool,panel:bool,stream:bool){
    let Some(root)=std::env::var_os("SKATE_GTA_ASSETS") else{return};
    std::thread::Builder::new().stack_size(1<<20).spawn(move||{
        let mut probe=Tube{panel};
        let mut ride=Ride::start(std::path::Path::new(&root),Mode::Easy,
            GtaVec::new(100.,200.,31.5),0.,PatchSettings::default(),&mut probe).unwrap();
        ride.native_collision=live;
        ride.find_walls=!live;
        let (mut streak,mut longest)=(0,0);
        let mut extended=false;
        let mut root_delta=(f32::MAX,f32::MIN);
        let mut deck_clearance=(f32::MAX,f32::MIN);
        for tick in 0..360 {
            let mut pad=[0.;18];
            if (30..120).contains(&tick){pad[16]=if (tick-30)%60<30{1.}else{0.};}
            if tick>=120 {
                let i=tick-120;
                pad[4]=match i{0..10=>-1.,10..14=>1.,_=>0.};
                if (8..40).contains(&i){pad[0]=1.;}
            }
            if live {
                ride.refresh_world(&mut probe).unwrap();
                ride.advance_native(Duration::from_secs_f32(1./60.),4,pad,&mut probe).unwrap();
                assert!(ride.obstacles().is_empty(),"live query mode still has obstacle boxes");
            }else{ride.advance(Duration::from_secs_f32(1./60.),4,pad,&mut probe).unwrap();}
            if ride.game.grinding(){
                if stream && !extended {
                    let mut lines:Vec<Vec<_>>=ride.grind_lines().iter().map(|line|
                        line.iter().map(|&p|ride.frame.to_skate(p)).collect()).collect();
                    for line in &mut lines {
                        let n=line.len();let a=line[n-2];let b=line[n-1];
                        let d=skate_core::math::Vector3::new(b.x-a.x,b.y-a.y,b.z-a.z);
                        let length=(d.x*d.x+d.y*d.y+d.z*d.z).sqrt();
                        if length>0.01{line.push(skate_core::math::Vector3::new(b.x+d.x*8./length,b.y+d.y*8./length,b.z+d.z*8./length));}
                    }
                    ride.game.set_grind_rails(&lines).unwrap();extended=true;
                }
                streak+=1;longest=longest.max(streak);
                let view=ride.view();
                if let Some(index)=ride.game.bone_names().iter().position(|n|n=="SKATEBOARD_ROOT"){
                    let delta=view.bones[index].0.z-view.deck.z;
                    root_delta.0=root_delta.0.min(delta);root_delta.1=root_delta.1.max(delta);
                }
                let deck=ride.deck_position();
                if (100.50..100.66).contains(&deck.x){
                    let delta=deck.z-31.95;
                    deck_clearance.0=deck_clearance.0.min(delta);deck_clearance.1=deck_clearance.1.max(delta);
                }
            }else{streak=0;}
        }
        eprintln!("thin rail grind: {longest} consecutive frames, deck {:?}",ride.deck_position());
        eprintln!("live={live}: animated root minus physical deck {root_delta:?}; deck above rail {deck_clearance:?}");
        assert!(!ride.grind_lines().is_empty(),"handrail has no grind spline");
        assert!(longest>=30,"handrail grind lasted only {longest} frames");
        assert!(!stream || extended,"no active grind was extended");
        assert!(root_delta.0.abs()<0.01 && root_delta.1.abs()<0.01,"rendered root diverged from physical deck");
        assert!(deck_clearance.0>0.,"deck sank below the rail while grinding");
    }).unwrap().join().unwrap();
}
