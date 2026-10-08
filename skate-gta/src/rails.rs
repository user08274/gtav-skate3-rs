//! Thin handrails found on GTA collision, never on a model bounding box.
use crate::{coords::{Frame, GtaVec}, terrain::{GroundProbe, mesh_triangles}};
use skate_core::physics::{board_world::WorldTriangle, contact::RetailContactMaterial};

#[derive(Clone)]
struct Rail {
    points: Vec<GtaVec>,
    width: f32,
}

struct Trace {
    points: Vec<GtaVec>,
    directions: [GtaVec; 2],
    width: f32,
    side: usize,
    steps: usize,
}

#[derive(Default)]
pub struct RailFinder {
    rails: Vec<Rail>,
    trace: Option<Trace>,
    next: usize,
    pub version: u64,
}

fn distance(a: GtaVec, b: GtaVec) -> f32 {
    let d = a.sub(b);
    (d.x*d.x + d.y*d.y + d.z*d.z).sqrt()
}
fn rail_distance(points:&[GtaVec],p:GtaVec)->f32{
    points.windows(2).map(|pair|{
        let a=pair[0];let d=pair[1].sub(a);let v=p.sub(a);
        let square=d.x*d.x+d.y*d.y+d.z*d.z;
        let t=if square>1e-8{((v.x*d.x+v.y*d.y+v.z*d.z)/square).clamp(0.,1.)}else{0.};
        distance(p,a.add(d.scale(t)))
    }).fold(f32::INFINITY,f32::min)
}

// Preserve bends within 5mm; quarter-metre trace samples on a straight bar
// must not become hundreds of redundant contact faces every physics tick.
fn simplify_trace(points:&[GtaVec])->Vec<GtaVec>{
    if points.len()<3{return points.to_vec();}
    let a=points[0];let b=*points.last().unwrap();let d=b.sub(a);
    let square=d.x*d.x+d.y*d.y+d.z*d.z;
    let mut worst=(0usize,0f32);
    for (i,&p) in points.iter().enumerate().skip(1).take(points.len()-2){
        let v=p.sub(a);let t=if square>1e-8{((v.x*d.x+v.y*d.y+v.z*d.z)/square).clamp(0.,1.)}else{0.};
        let error=distance(p,a.add(d.scale(t)));
        if error>worst.1{worst=(i,error);}
    }
    if worst.1<=0.005{return vec![a,b];}
    let mut out=simplify_trace(&points[..=worst.0]);out.pop();
    out.extend(simplify_trace(&points[worst.0..]));out
}

impl RailFinder {
    /// Bounded work: at most 64 native ray queries per update. A trace resumes
    /// next frame rather than blocking the script fiber to finish a long rail.
    pub fn update(&mut self, probe: &mut dyn GroundProbe, center: GtaVec) {
        self.update_inner(probe,center,None);
    }
    pub fn update_ahead(&mut self,probe:&mut dyn GroundProbe,center:GtaVec,forward:GtaVec){
        self.update_inner(probe,center,Some(forward));
    }
    fn update_inner(&mut self,probe:&mut dyn GroundProbe,center:GtaVec,forward:Option<GtaVec>){
        let mut budget = 64usize;
        if let Some(mut trace) = self.trace.take() {
            while budget >= 7 && trace.side < 2 {
                budget -= 7;
                let last = if trace.side == 0 { *trace.points.last().unwrap() } else { trace.points[0] };
                let direction = trace.directions[trace.side];
                let expected = last.add(direction.scale(0.25));
                let across = GtaVec::new(direction.y, -direction.x, 0.).normalized();
                let mut hits = Vec::new();
                for i in -3..=3 {
                    let at = expected.add(across.scale(i as f32 * 0.05));
                    if let Some(z) = probe.down(at.x, at.y, expected.z + 0.3, expected.z - 0.3) {
                        hits.push(GtaVec::new(at.x, at.y, z));
                    }
                }
                // A broad platform is a ledge; a single upright is not a rail.
                if hits.is_empty() || hits.len() >= 7 || trace.steps >= 128 {
                    trace.side += 1;
                    trace.steps = 0;
                    continue;
                }
                let a = hits[0]; let b = *hits.last().unwrap();
                let at = a.lerp(b, 0.5);
                if (at.z - last.z).abs() > 0.375 {
                    trace.side += 1; trace.steps = 0; continue;
                }
                let motion=at.sub(last).normalized();
                if motion.x*direction.x+motion.y*direction.y<0.5 {
                    trace.side+=1; trace.steps=0; continue;
                }
                trace.directions[trace.side]=direction.lerp(motion,0.6).normalized();
                if trace.side == 0 { trace.points.push(at); } else { trace.points.insert(0, at); }
                trace.steps += 1;
            }
            if trace.side == 2 {
                if trace.points.len() >= 5 && distance(trace.points[0], *trace.points.last().unwrap()) >= 1.0 {
                    self.rails.push(Rail { points: simplify_trace(&trace.points), width: trace.width });
                    self.version += 1;
                }
            } else { self.trace = Some(trace); }
        }
        if self.trace.is_none() && budget >= 18 {
            // Dither height over successive sweeps so a four-centimetre tube
            // cannot permanently sit between the horizontal sample levels.
            for _ in 0..4 {
                if budget < 18 { break; }
                budget -= 18;
                let k = self.next; self.next += 1;
                let base=forward.map(|d|d.y.atan2(d.x)).unwrap_or(0.);
                let (angle,height)=if forward.is_some(){
                    let sweep=k/3;
                    let offset=match k%3 {0=>0.,1=>std::f32::consts::TAU/16.,_=>-std::f32::consts::TAU/16.};
                    (base+offset,0.25+(sweep%29) as f32*0.1+((sweep/29)%4) as f32*0.025)
                }else{
                    (base+(k%16) as f32*std::f32::consts::TAU/16.,
                        0.25+((k/16)%29) as f32*0.1+((k/(16*29))%4) as f32*0.025)
                };
                let from = center.add(GtaVec::new(0., 0., height));
                let reach=if forward.is_some(){10.}else{3.};
                let to = from.add(GtaVec::new(angle.cos()*reach, angle.sin()*reach, 0.));
                let Some((hit, normal)) = probe.toward(from, to) else { continue };
                if normal.z.abs() > 0.7 { continue; }
                if self.rails.iter().any(|r| rail_distance(&r.points,hit)<0.4) { continue; }
                let n = GtaVec::new(normal.x, normal.y, 0.).normalized();
                let inside = hit.sub(n.scale(0.01));
                let Some(top) = probe.down(inside.x, inside.y, hit.z + 0.3, hit.z - 0.08) else { continue };
                let mut last = inside;
                let mut outer = None;
                for i in 1..=8 {
                    let at = hit.sub(n.scale(0.01 + i as f32 * 0.04));
                    if probe.down(at.x, at.y, top + 0.08, top - 0.08).is_some() { last = at; }
                    else { outer = Some(at); break; }
                }
                let Some(mut outer) = outer else { continue };
                for _ in 0..3 {
                    let mid = last.lerp(outer, 0.5);
                    if probe.down(mid.x, mid.y, top + 0.08, top - 0.08).is_some() { last = mid; }
                    else { outer = mid; }
                }
                let far = last.lerp(outer, 0.5);
                let width = distance(hit, GtaVec::new(far.x, far.y, hit.z));
                if !(0.02..=0.3).contains(&width) { continue; }
                // Broad wall caps have two ledge edges. A metal fence may
                // have a thin collision panel under its grindable top bar.
                let below=|p:GtaVec|GtaVec::new(p.x,p.y,top-0.18);
                // One upright beneath a top tube is a support, not a solid
                // wall. A sheet fills all three cross-sections along it.
                let tangent=GtaVec::new(-n.y,n.x,0.);
                if width>0.12 && [-0.2,0.,0.2].into_iter().all(|offset|{
                    let shift=tangent.scale(offset);
                    probe.toward(below(hit.add(n.scale(0.1)).add(shift)),below(far.sub(n.scale(0.1)).add(shift))).is_some()
                }){continue;}
                // Leaving a narrow height band does not prove a tube ends:
                // a steep ramp simply rises above that band's ray origin.
                // Look higher outside both sides before accepting open air.
                let outside=[hit.add(n.scale(0.1)),far.sub(n.scale(0.1))];
                if outside.iter().any(|p| probe.down(p.x,p.y,top+1.8,top-0.12).is_some()) {
                    continue;
                }
                let p = GtaVec::new((hit.x + far.x)*0.5, (hit.y + far.y)*0.5, top);
                let mut tangent=GtaVec::new(-n.y,n.x,0.);
                if forward.is_some_and(|f|f.x*tangent.x+f.y*tangent.y<0.){tangent=tangent.scale(-1.);}
                self.trace = Some(Trace { points: vec![p], directions: [tangent,tangent.scale(-1.)], width, side: 0, steps: 0 });
                break;
            }
        }
        let before = self.rails.len();
        self.rails.retain(|r| rail_distance(&r.points,center)<40.);
        if self.rails.len() != before { self.version += 1; }
    }

    pub fn lines(&self, center: GtaVec) -> Vec<Vec<GtaVec>> {
        self.rails.iter().filter(|r| rail_distance(&r.points,center)<15.)
            .map(|r| r.points.clone()).collect()
    }

    /// Only the ray-confirmed top tube becomes collision, leaving fence gaps
    /// and the air under street-light arms open.
    pub fn triangles(&self, frame: &Frame, center: GtaVec, material: RetailContactMaterial) -> Vec<WorldTriangle> {
        let mut out = Vec::new();
        for rail in &self.rails {
            if rail_distance(&rail.points,center)>=15. { continue; }
            let mut points = Vec::new();
            for (i, &p) in rail.points.iter().enumerate() {
                let a = rail.points[i.saturating_sub(1)];
                let b = rail.points[(i + 1).min(rail.points.len() - 1)];
                let d = b.sub(a);
                let along = d.normalized();
                let across = GtaVec::new(along.y, -along.x, 0.).normalized();
                let up = GtaVec::new(across.y*along.z, -across.x*along.z,
                    across.x*along.y - across.y*along.x).normalized();
                let left = p.sub(across.scale(rail.width*0.5));
                let right = p.add(across.scale(rail.width*0.5));
                for v in [left, right, right.sub(up.scale(0.04)), left.sub(up.scale(0.04))] {
                    points.push(frame.to_skate(v));
                }
            }
            let mut faces = vec![[0,3,2], [0,2,1]];
            for i in 0..rail.points.len() - 1 {
                for side in 0..4 {
                    let a = i*4 + side; let b = i*4 + (side+1)%4;
                    faces.extend([[a,b,b+4], [a,b+4,a+4]]);
                }
            }
            let end = points.len() - 4;
            faces.extend([[end,end+1,end+2], [end,end+2,end+3]]);
            out.extend(mesh_triangles(&points, &faces, material).into_iter().map(|mut t|{
                t.tag=skate_gameplay::host::HOST_RAIL_TAG;t
            }));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]fn every_search_update_casts_a_ten_metre_forward_ray(){
        struct Miss(Vec<(GtaVec,GtaVec)>);
        impl GroundProbe for Miss {
            fn down(&mut self,_x:f32,_y:f32,_top:f32,_bottom:f32)->Option<f32>{None}
            fn toward(&mut self,a:GtaVec,b:GtaVec)->Option<(GtaVec,GtaVec)>{self.0.push((a,b));None}
        }
        let mut finder=RailFinder::default();let mut probe=Miss(Vec::new());
        for _ in 0..90 {
            probe.0.clear();finder.update_ahead(&mut probe,GtaVec::default(),GtaVec::new(0.,1.,0.));
            assert!(probe.0.iter().any(|(a,b)|{let d=b.sub(*a);d.x.abs()<0.001 && (d.y-10.).abs()<0.001}),"forward ray omitted");
            assert!(probe.0.len()<=3);
        }
    }

    #[test]
    fn a_thin_fence_panel_has_a_grindable_top() {
        struct Fence(Tube);
        impl GroundProbe for Fence {
            fn down(&mut self,x:f32,y:f32,top:f32,bottom:f32)->Option<f32>{self.0.down(x,y,top,bottom)}
            fn toward(&mut self,a:GtaVec,b:GtaVec)->Option<(GtaVec,GtaVec)>{
                let d=b.sub(a);
                if d.x>0. {
                    let t=(1.013-a.x)/d.x;let p=a.add(d.scale(t));
                    if (0. ..=1.).contains(&t) && p.y.abs()<=4. && (0. ..=1.037).contains(&p.z){
                        return Some((p,GtaVec::new(-1.,0.,0.)));
                    }
                }
                self.0.toward(a,b)
            }
        }
        let center=GtaVec::new(0.,0.,0.087);let mut finder=RailFinder::default();
        for _ in 0..400{finder.update(&mut Fence(Tube{slope:0.,calls:0}),center);}
        assert!(!finder.lines(center).is_empty(),"solid fence collision below a narrow handrail hid its top");
        assert!(!finder.triangles(&Frame::new(GtaVec::default()),center,
            RetailContactMaterial{static_friction:0.,dynamic_friction:0.,restitution:1.}).is_empty());
    }

    #[test]fn a_long_straight_trace_needs_only_two_contact_sections(){
        let points:Vec<_>=(0..257).map(|i|GtaVec::new(0.,i as f32*0.25,i as f32*0.1)).collect();
        let simple=simplify_trace(&points);assert_eq!(simple,vec![points[0],points[256]]);
    }

    struct HighCurve;
    impl GroundProbe for HighCurve {
        fn down(&mut self,x:f32,y:f32,top:f32,bottom:f32)->Option<f32> {
            let dx=x-4.; let r=(dx*dx+y*y).sqrt();
            let z=if (3.95..=4.05).contains(&r) && y.atan2(-dx).abs()<1. {
                2.7
            }else{0.};
            (top>=z && bottom<=z).then_some(z)
        }
        fn toward(&mut self,from:GtaVec,to:GtaVec)->Option<(GtaVec,GtaVec)> {
            if !(2.61..=2.7).contains(&from.z){return None;}
            let d=to.sub(from); let x=from.x-4.; let y=from.y;
            let a=d.x*d.x+d.y*d.y;
            if a<1e-6{return None;}
            let b=2.*(x*d.x+y*d.y);
            let mut best:Option<(f32,GtaVec,GtaVec)>=None;
            for r in [3.95f32,4.05] {
                let c=x*x+y*y-r*r; let discriminant=b*b-4.*a*c;
                if discriminant<0.{continue;}
                for t in [(-b-discriminant.sqrt())/(2.*a),(-b+discriminant.sqrt())/(2.*a)] {
                    let p=from.add(d.scale(t));
                    if !(0. ..=1.).contains(&t) || p.y.atan2(4.-p.x).abs()>=1.{continue;}
                    let n=GtaVec::new(p.x-4.,p.y,0.).normalized().scale(if r<4.{-1.}else{1.});
                    if best.is_none_or(|v|t<v.0){best=Some((t,p,n));}
                }
            }
            best.map(|(_,p,n)|(p,n))
        }
    }

    #[test]
    fn a_high_curved_handrail_is_traced_around_its_bend() {
        let mut finder=RailFinder::default();
        let center=GtaVec::new(-0.8,0.,0.087);
        for _ in 0..1600 {finder.update(&mut HighCurve,center);}
        let lines=finder.lines(center);
        assert!(!lines.is_empty(),"rail 2.7 m above lower ground was missed");
        assert!(lines.iter().any(|line|line.windows(2).map(|w|distance(w[0],w[1])).sum::<f32>()>6.),
            "curved tube was reduced to short straight pieces");
        for p in lines.iter().flatten() {
            let r=((p.x-4.).powi(2)+p.y*p.y).sqrt();
            assert!((r-4.).abs()<0.06 && (p.z-2.7).abs()<0.005,"off tube: {p:?}");
        }
        assert!(!finder.triangles(&Frame::new(GtaVec::default()),center,
            RetailContactMaterial{static_friction:0.5,dynamic_friction:0.5,restitution:0.}).is_empty());
    }
    struct Tube { slope: f32, calls: usize }
    #[test]fn a_parallel_rail_is_seeded_quickly_and_traced_forward_first(){
        let mut finder=RailFinder::default();let mut tube=Tube{slope:0.,calls:0};
        for _ in 0..20 {
            finder.update_ahead(&mut tube,GtaVec::new(0.,0.,0.087),GtaVec::new(0.,1.,0.));
            if let Some(trace)=&finder.trace{
                assert!(trace.directions[0].y>0.9,"tracing spent its first budget behind the skater");return;
            }
        }
        panic!("forward side-angle sweep missed a parallel tube");
    }
    #[test]fn a_supported_handrail_eight_metres_ahead_is_found(){
        struct Supported(Tube);
        impl GroundProbe for Supported {
            fn down(&mut self,x:f32,y:f32,top:f32,bottom:f32)->Option<f32>{self.0.down(x-7.,y,top,bottom)}
            fn toward(&mut self,a:GtaVec,b:GtaVec)->Option<(GtaVec,GtaVec)>{
                let shift=GtaVec::new(7.,0.,0.);let local=a.sub(shift);let end=b.sub(shift);let d=end.sub(local);
                if d.x>0. && local.z<0.9 {
                    let t=(1.013-local.x)/d.x;let p=local.add(d.scale(t));
                    if (0. ..=1.).contains(&t) && p.y.abs()<0.06 && p.z>0. {
                        return Some((p.add(shift),GtaVec::new(-1.,0.,0.)));
                    }
                }
                self.0.toward(local,end).map(|(p,n)|(p.add(shift),n))
            }
        }
        let center=GtaVec::new(0.,0.,0.087);let mut finder=RailFinder::default();
        let mut probe=Supported(Tube{slope:0.,calls:0});
        for _ in 0..400{finder.update_ahead(&mut probe,center,GtaVec::new(1.,0.,0.));}
        let lines=finder.lines(center);
        assert!(!lines.is_empty(),"supported rail beyond old 3m range was missed");
        assert!(lines.iter().flatten().all(|p|(p.x-8.043).abs()<0.04));
    }
    impl GroundProbe for Tube {
        fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
            self.calls += 1;
            let floor = self.slope*y;
            let z = if (1.013..=1.073).contains(&x) && y.abs() <= 4. {
                floor + 1.037
            } else { floor };
            (top >= z && bottom <= z).then_some(z)
        }
        fn toward(&mut self, from: GtaVec, to: GtaVec) -> Option<(GtaVec,GtaVec)> {
            self.calls += 1;
            let d = to.sub(from);
            if d.x <= 0. { return None; }
            let t = (1.013 - from.x)/d.x;
            if !(0. ..=1.).contains(&t) { return None; }
            let hit = from.add(d.scale(t));
            let top = self.slope*hit.y + 1.037;
            (hit.y.abs() <= 4. && hit.z <= top && hit.z >= top - 0.06)
                .then_some((hit,GtaVec::new(-1.,0.,0.)))
        }
    }
    #[test]
    fn a_steep_sheet_is_not_a_thin_rail() {
        struct Ramp;
        impl GroundProbe for Ramp {
            fn down(&mut self,x:f32,_y:f32,top:f32,bottom:f32)->Option<f32> {
                let z=3.0*x;
                (top>=z && bottom<=z).then_some(z)
            }
            fn toward(&mut self,from:GtaVec,to:GtaVec)->Option<(GtaVec,GtaVec)> {
                let d=to.sub(from);
                let denominator=d.z-3.0*d.x;
                if denominator.abs()<1e-6{return None;}
                let t=(3.0*from.x-from.z)/denominator;
                (0. ..=1.).contains(&t).then_some((from.add(d.scale(t)),GtaVec::new(-3.0,0.,1.).normalized()))
            }
        }
        let mut finder=RailFinder::default();
        for _ in 0..300{finder.update(&mut Ramp,GtaVec::new(0.,0.,0.087));}
        assert!(finder.lines(GtaVec::default()).is_empty(),"ramp sheet became a tube");
    }

    #[test]
    fn thin_flat_and_stair_rails_get_a_real_top_line_with_bounded_queries() {
        for slope in [0., 0.15, 0.4, 0.65, 0.9] {
            let mut tube = Tube { slope, calls: 0 };
            let mut finder = RailFinder::default();
            let center = GtaVec::new(0.,0.,0.087);
            for _ in 0..300 {
                tube.calls = 0;
                finder.update(&mut tube,center);
                assert!(tube.calls <= 64, "ray budget exceeded: {}", tube.calls);
                if !finder.lines(center).is_empty() { break; }
            }
            let lines = finder.lines(center);
            assert!(!lines.is_empty(), "thin rail was missed, slope {slope}");
            let line = &lines[0];
            assert!(distance(line[0], *line.last().unwrap()) > 7.5, "rail was truncated: {line:?}");
            for p in line {
                assert!((p.x - 1.043).abs() < 0.03);
                assert!((p.z - (slope*p.y + 1.037)).abs() < 0.005);
            }
            let triangles = finder.triangles(&Frame::new(GtaVec::default()),center,
                RetailContactMaterial { static_friction:0.,dynamic_friction:0.,restitution:1. });
            assert!(!triangles.is_empty(), "rail must have collision as well as a grind spline");
            // Native geometry investigation sees an actual thin rail.
            use skate_core::air::trajectory::grind_surface::{self, InvestigationInput, ProbeHit, GeometryType};
            use skate_core::physics::triangle_query::{TriangleLineHit, triangle_segment};
            use skate_core::math::Vector3;
            let at = line[line.len()/2];
            let v = |p:GtaVec| [p.x,p.z,-p.y,0.];
            let surface = grind_surface::investigate(InvestigationInput {
                start:v(line[0]),end:v(*line.last().unwrap()),reference:v(at),optional_probe:None,deck_center_to_truck:0.234
            }, |_, probe| -> Result<Option<ProbeHit>,String> {
                let start = Vector3::new(probe.start[0],probe.start[1],probe.start[2]);
                let delta = Vector3::new(probe.end[0]-probe.start[0],probe.end[1]-probe.start[1],probe.end[2]-probe.start[2]);
                let mut nearest: Option<ProbeHit> = None;
                for t in &triangles {
                    let mut hit = TriangleLineHit { position:Vector3::ZERO,normal:Vector3::ZERO,fraction:0.,volume_parameter:[0.;3] };
                    if triangle_segment(&mut hit,start,delta,t.triangle.vertices,probe.radius,0.)
                        && nearest.is_none_or(|h|hit.fraction<h.fraction) {
                        nearest=Some(ProbeHit { fraction:hit.fraction,position:[hit.position.x,hit.position.y,hit.position.z,0.],
                            normal:[hit.normal.x,hit.normal.y,hit.normal.z,0.],packed_surface:0 });
                    }
                }
                Ok(nearest)
            }).unwrap();
            assert_eq!(surface.kind, GeometryType::ThinRail);
        }
    }
}
