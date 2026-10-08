//! Actual host contacts must enter a bowl and follow a curved transition.
use skate_gameplay::host::Mode;
use skate_gta::{coords::GtaVec,ride::Ride,terrain::{GroundProbe,PatchSettings}};
use std::time::Duration;
struct Transition { direction:f32 }
impl Transition {
    fn height(&self,y:f32)->f32 {
        let u=(y-206.).clamp(0.,1.95);
        31.5+self.direction*(2.-(4.-u*u).sqrt())
    }
}
impl GroundProbe for Transition {
    fn down(&mut self,_x:f32,y:f32,top:f32,bottom:f32)->Option<f32>{
        let z=self.height(y);(top>=z && bottom<=z).then_some(z)
    }
    fn toward(&mut self,a:GtaVec,b:GtaVec)->Option<(GtaVec,GtaVec)>{
        let d=b.sub(a);let signed=|t:f32|{let p=a.add(d.scale(t));p.z-self.height(p.y)};
        let mut lo=0.;let mut value=signed(lo);
        for i in 1..=16 {
            let mut hi=i as f32/16.;let next=signed(hi);
            if value*next<0. {
                for _ in 0..14 {let mid=(lo+hi)*0.5;if signed(mid)*value>0.{lo=mid;}else{hi=mid;}}
                let p=a.add(d.scale((lo+hi)*0.5));let u=(p.y-206.).clamp(0.,1.95);
                let slope=if p.y>206. && p.y<207.95{self.direction*u/(4.-u*u).sqrt()}else{0.};
                return Some((p,GtaVec::new(0.,-slope,1.).normalized()));
            }
            lo=hi;value=next;
        }
        None
    }
}
#[test]fn native_contacts_enter_a_bowl_and_climb_a_transition(){
    let Some(root)=std::env::var_os("SKATE_GTA_ASSETS") else{return};
    std::thread::Builder::new().stack_size(1<<20).spawn(move||{
        for direction in [-1.,1.] {
            let mut probe=Transition{direction};
            let mut ride=Ride::start(std::path::Path::new(&root),Mode::Easy,GtaVec::new(100.,200.,31.5),0.,PatchSettings::default(),&mut probe).unwrap();
            ride.native_collision=true;ride.find_walls=false;
            let (mut furthest,mut highest,mut lowest_clearance)=(200.0f32,31.5f32,f32::MAX);
            for i in 0..300 {
                let mut pad=[0.;18];pad[16]=if i<210 && i%60<30{1.}else{0.};
                ride.refresh_world(&mut probe).unwrap();
                ride.advance_native(Duration::from_secs_f32(1./60.),4,pad,&mut probe).unwrap();
                let deck=ride.deck_position();
                furthest=furthest.max(deck.y);highest=highest.max(deck.z);
                if i>30 {lowest_clearance=lowest_clearance.min(deck.z-probe.height(deck.y));}
            }
            eprintln!("native transition direction={direction}: furthest={furthest} highest={highest} clearance={lowest_clearance}");
            assert!(lowest_clearance> -0.08,"board sank through native transition");
            if direction<0. {assert!(furthest>210.,"stopped entering the bowl: {furthest}");}
            else {
                // Uphill velocity falls under gravity; matching the sampled
                // mesh's distant finish would require artificial acceleration.
                assert!(furthest>207.1 && highest>32.,"blocked at ramp entrance: y={furthest} z={highest}");
            }
        }
    }).unwrap().join().unwrap();
}

struct Hill;
impl GroundProbe for Hill {
    fn down(&mut self,_x:f32,y:f32,top:f32,bottom:f32)->Option<f32>{
        let z=31.5+(y-206.).max(0.)*0.12;(top>=z && bottom<=z).then_some(z)
    }
    fn toward(&mut self,a:GtaVec,b:GtaVec)->Option<(GtaVec,GtaVec)>{
        let d=b.sub(a);let mut hits=Vec::new();
        for slope in [0.,0.12] {
            let divisor=d.z-slope*d.y;if divisor.abs()<1e-7{continue;}
            let t=(31.5+slope*(a.y-206.)-a.z)/divisor;let p=a.add(d.scale(t));
            if (0. ..=1.).contains(&t) && ((slope==0. && p.y<=206.) || (slope>0. && p.y>=206.)){
                hits.push((t,p,GtaVec::new(0.,-slope,1.).normalized()));
            }
        }
        hits.into_iter().min_by(|a,b|a.0.total_cmp(&b.0)).map(|(_,p,n)|(p,n))
    }
}
#[test]fn native_contacts_roll_up_a_gentle_hill_without_a_barrier(){
    let Some(root)=std::env::var_os("SKATE_GTA_ASSETS") else{return};
    std::thread::Builder::new().stack_size(1<<20).spawn(move||{
        let mut probe=Hill;
        let mut ride=Ride::start(std::path::Path::new(&root),Mode::Easy,GtaVec::new(100.,200.,31.5),0.,PatchSettings::default(),&mut probe).unwrap();
        ride.native_collision=true;ride.find_walls=false;
        for i in 0..300 {
            let mut pad=[0.;18];pad[16]=if i%60<30{1.}else{0.};
            ride.refresh_world(&mut probe).unwrap();
            ride.advance_native(Duration::from_secs_f32(1./60.),4,pad,&mut probe).unwrap();
        }
        let p=ride.deck_position();eprintln!("native gentle hill: {p:?}");
        assert!(p.y>215. && p.z>32.5,"stopped at a continuous hill: {p:?}");
    }).unwrap().join().unwrap();
}
