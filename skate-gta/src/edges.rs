//! Grindable edges found in GTA's ground: curbs, ledges and step noses.
//!
//! GTA only answers rays, so the finder keeps a world-aligned grid of
//! downward samples around the skater. Neighbouring samples whose heights
//! differ by a ledge-like step are refined by bisection with a few more rays
//! until the step is located to under a centimetre; a slope fails that test
//! (its midpoints are neither height). The edge points along the top corner
//! are then joined into polylines, which become Skate 3 grind splines.
use crate::{coords::GtaVec, terrain::GroundProbe};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug)]
pub struct EdgeSettings {
    /// Grid spacing of the coarse samples, metres.
    pub spacing: f32,
    /// Sampling radius around the skater, metres.
    pub radius: f32,
    /// Height differences treated as a grindable step (curb up to waist-high ledge).
    pub min_step: f32,
    pub max_step: f32,
    /// Rays allowed per update.
    pub rays: usize,
    /// Ray allowance for bisection and the two plateau checks at a bevel.
    pub refine_steps: u32,
}

impl Default for EdgeSettings {
    fn default() -> Self {
        Self { spacing: 0.25, radius: 6.0, min_step: 0.03, max_step: 1.8, rays: 64, refine_steps: 17 }
    }
}

/// A point on the top corner of a step and the horizontal direction down it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgePoint {
    pub position: GtaVec,
    pub drop: GtaVec,
}

type Node = (i32, i32);

pub struct EdgeFinder {
    pub settings: EdgeSettings,
    heights: HashMap<Node, Option<f32>>,
    checked: HashSet<(Node, u8)>,
    points: Vec<EdgePoint>,
    updates: u32,
    /// Bumped whenever an edge point is added.
    pub version: u64,
}

fn length2(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

impl EdgeFinder {
    pub fn new(settings: EdgeSettings) -> Self {
        Self { settings, heights: HashMap::new(), checked: HashSet::new(), points: Vec::new(), updates: 0, version: 0 }
    }

    pub fn points(&self) -> &[EdgePoint] {
        &self.points
    }

    fn world(&self, node: Node) -> (f32, f32) {
        (node.0 as f32 * self.settings.spacing, node.1 as f32 * self.settings.spacing)
    }

    fn ray(probe: &mut dyn GroundProbe, x: f32, y: f32, around: f32) -> Option<f32> {
        probe.down(x, y, around + 3.0, around - 4.0)
    }

    /// Samples and refines around `center` within the ray budget; returns
    /// whether new edge points were found.
    pub fn update(&mut self, probe: &mut dyn GroundProbe, center: GtaVec) -> bool {
        self.update_inner(probe,center,None)
    }
    pub fn update_ahead(&mut self,probe:&mut dyn GroundProbe,origin:GtaVec,forward:GtaVec)->bool{
        let direction=GtaVec::new(forward.x,forward.y,0.).normalized();
        self.update_inner(probe,origin.add(direction.scale(5.)),Some((origin,direction)))
    }
    fn update_inner(&mut self, probe:&mut dyn GroundProbe,center:GtaVec,ahead:Option<(GtaVec,GtaVec)>)->bool {
        let s = self.settings;
        self.updates += 1;
        if self.updates % 30 == 0 {
            // An unloaded or temporarily missing collision sample is retryable.
            let missing: HashSet<_> = self.heights.iter().filter_map(|(&n,h)| h.is_none().then_some(n)).collect();
            self.heights.retain(|n,_| !missing.contains(n));
            self.checked.retain(|&(n,dir)| {
                let other=if dir==0{(n.0+1,n.1)}else{(n.0,n.1+1)};
                !missing.contains(&n) && !missing.contains(&other)
            });
        }
        let mut rays = 0usize;
        let reach = (s.radius / s.spacing).ceil() as i32;
        let (cx, cy) = ((center.x / s.spacing).round() as i32, (center.y / s.spacing).round() as i32);
        // Nearest nodes first.
        let mut nodes: Vec<Node> = (-reach..=reach)
            .flat_map(|dx| (-reach..=reach).map(move |dy| (dx, dy)))
            .filter(|&(dx, dy)| (dx * dx + dy * dy) as f32 <= (reach * reach) as f32)
            .map(|(dx, dy)| (cx + dx, cy + dy))
            .collect();
        nodes.sort_by_key(|&(x, y)| (x - cx).pow(2) + (y - cy).pow(2));
        if let Some((origin,forward))=ahead {
            // Fill the corridor to ten metres before spending rays on the
            // surrounding disk. Far-first ordering keeps its leading end fresh.
            nodes.sort_by_key(|&node|{
                let (x,y)=self.world(node);let dx=x-origin.x;let dy=y-origin.y;
                let along=dx*forward.x+dy*forward.y;
                let across=(dx*forward.y-dy*forward.x).abs();
                if (0. ..=10.25).contains(&along) && across<=0.75 {
                    (0,(10000.-along*1000.) as i32,(across*1000.) as i32)
                }else{(1,(((x-center.x).powi(2)+(y-center.y).powi(2))*1000.) as i32,0)}
            });
        }
        for &node in &nodes {
            if rays >= s.rays / 2 {
                break;
            }
            if !self.heights.contains_key(&node) {
                let (x, y) = self.world(node);
                let h = Self::ray(probe, x, y, center.z);
                self.heights.insert(node, h);
                rays += 1;
            }
        }
        let before = self.points.len();
        for &node in &nodes {
            for dir in 0..2u8 {
                if self.checked.contains(&(node, dir)) {
                    continue;
                }
                let other = if dir == 0 { (node.0 + 1, node.1) } else { (node.0, node.1 + 1) };
                let (Some(&a), Some(&b)) = (self.heights.get(&node), self.heights.get(&other)) else { continue };
                let step = match (a, b) {
                    (Some(a), Some(b)) => (a - b).abs(),
                    _ => 0.0,
                };
                if step < s.min_step || step > s.max_step {
                    self.checked.insert((node, dir));
                    continue;
                }
                if rays + s.refine_steps as usize > s.rays {
                    continue;
                }
                self.checked.insert((node, dir));
                let (a, b) = (a.unwrap(), b.unwrap());
                let (high, low, hh, hl) = if a > b { (node, other, a, b) } else { (other, node, b, a) };
                rays += s.refine_steps as usize;
                if let Some(point) = self.refine(probe, high, low, hh, hl, center.z) {
                    self.points.push(point);
                }
            }
        }
        // Forget samples far behind so memory stays bounded.
        if self.heights.len() > 40_000 {
            let keep = (reach * 3) as i64;
            let far = |&(x, y): &Node| ((x - cx) as i64).abs() > keep || ((y - cy) as i64).abs() > keep;
            self.heights.retain(|n, _| !far(n));
            self.checked.retain(|(n, _)| !far(n));
            let limit = s.radius * 3.0;
            self.points.retain(|p| length2(p.position.x - center.x, p.position.y - center.y) < limit);
            self.version += 1;
        }
        if self.points.len() > before {
            self.version += 1;
            true
        } else {
            false
        }
    }

    /// Bisection between a high and a low sample: the step must switch from
    /// one height straight to the other.
    fn refine(&self, probe: &mut dyn GroundProbe, high: Node, low: Node, hh: f32, hl: f32, _around: f32) -> Option<EdgePoint> {
        let (a, b) = (self.world(high), self.world(low));
        let (point, bottom) = crate::terrain::step_between(probe, GtaVec::new(a.0, a.1, hh), GtaVec::new(b.0, b.1, hl))?;
        let len = length2(b.0 - a.0, b.1 - a.1);
        let mut drop = GtaVec::new((b.0 - a.0) / len, (b.1 - a.1) / len, 0.0);
        // A ledge needs a rideable top. Collision seams on a steep bowl sheet
        // may survive bisection, but the surface keeps climbing behind them.
        let mut previous=point;
        for distance in [0.075,0.15,0.225,0.3] {
            let at=point.sub(drop.scale(distance));
            let z=probe.down(at.x,at.y,point.z+0.6,point.z-0.6)?;
            // After confirmed flat support, a drop is the opposite corner
            // of a narrow cap. Continued ascent still rejects bowl seams.
            if z-previous.z>0.075*0.6 {return None;}
            if previous.z-z>0.075*0.6 {
                if distance>0.075 {break;} else {return None;}
            }
            previous=GtaVec::new(at.x,at.y,z);
        }
        let at=GtaVec::new(point.x,point.y,(point.z+bottom.z)*0.5);
        if let Some((_,normal))=probe.toward(at.add(drop.scale(0.1)),at.sub(drop.scale(0.1))) {
            if normal.z.abs()<0.3 {drop=GtaVec::new(normal.x,normal.y,0.).normalized();}
        }
        Some(EdgePoint { position: point, drop })
    }

    /// Edge points near `center` joined into polylines along the step tops.
    pub fn polylines(&self, center: GtaVec, radius: f32) -> Vec<Vec<GtaVec>> {
        // Finer discovery must not reduce the tolerated distance between
        // confirmed points on a curved cap. Keep the previous 80cm limit.
        let link = (self.settings.spacing * 1.6).max(0.8);
        let near: Vec<&EdgePoint> = self
            .points
            .iter()
            .filter(|p| length2(p.position.x - center.x, p.position.y - center.y) <= radius*3.)
            .collect();
        // Union-find over points close in plan and height, dropping the same way.
        let mut parent: Vec<usize> = (0..near.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for i in 0..near.len() {
            for j in i + 1..near.len() {
                let (p, q) = (near[i], near[j]);
                let delta=q.position.sub(p.position);
                let separation=length2(delta.x,delta.y);
                let close = length2(p.position.x - q.position.x, p.position.y - q.position.y) <= link
                    && (p.position.z - q.position.z).abs() <= 0.05
                    && p.drop.x * q.drop.x + p.drop.y * q.drop.y > 0.5
                    && (delta.x*p.drop.x+delta.y*p.drop.y).abs()
                        .min((delta.x*q.drop.x+delta.y*q.drop.y).abs())<0.06+separation*0.2;
                if close {
                    let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                    parent[a] = b;
                }
            }
        }
        let mut groups: HashMap<usize, Vec<GtaVec>> = HashMap::new();
        for i in 0..near.len() {
            let r = root(&mut parent, i);
            groups.entry(r).or_default().push(near[i].position);
        }
        let mut lines = Vec::new();
        let mut keys: Vec<usize> = groups.keys().copied().collect();
        keys.sort_unstable();
        for key in keys {
            let points = &groups[&key];
            if points.len() < 3 || !points.iter().any(|p| length2(p.x-center.x,p.y-center.y)<=radius) {
                continue;
            }
            for piece in order_along(points, link) {
                let length: f32 = piece.windows(2).map(|w| length2(w[1].x - w[0].x, w[1].y - w[0].y)).sum();
                if piece.len() >= 3 && length >= 1.0 {
                    lines.push(simplify(&piece, 0.03));
                }
            }
        }
        lines
    }
}

/// Orders points along their main direction and splits at gaps and sharp turns.
fn order_along(points: &[GtaVec], gap: f32) -> Vec<Vec<GtaVec>> {
    let mut remaining=points.to_vec();
    let mut pieces=Vec::new();
    while !remaining.is_empty() {
        let mean=remaining.iter().fold(GtaVec::default(),|a,p|a.add(*p)).scale(1./remaining.len() as f32);
        let start=remaining.iter().enumerate().max_by(|(_,a),(_,b)|
            length2(a.x-mean.x,a.y-mean.y).total_cmp(&length2(b.x-mean.x,b.y-mean.y)))
            .map(|(i,_)|i).unwrap();
        let mut piece=vec![remaining.remove(start)];
        loop {
            let last=*piece.last().unwrap();
            let next=remaining.iter().enumerate().filter(|(_,p)|{
                let distance=length2(p.x-last.x,p.y-last.y);
                if distance>gap{return false;}
                if piece.len()<2{return true;}
                let previous=piece[piece.len()-2];
                let a=last.sub(previous);let b=p.sub(last);
                (a.x*b.x+a.y*b.y)/(length2(a.x,a.y)*distance).max(1e-6)>0.3
            }).min_by(|(_,a),(_,b)|length2(a.x-last.x,a.y-last.y).total_cmp(&length2(b.x-last.x,b.y-last.y)))
                .map(|(i,_)|i);
            let Some(next)=next else{break};
            piece.push(remaining.remove(next));
        }
        pieces.push(piece);
    }
    pieces
}
/// Ramer-Douglas-Peucker in plan view (heights follow the kept points).
fn simplify(points: &[GtaVec], tolerance: f32) -> Vec<GtaVec> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let (a, b) = (points[0], points[points.len() - 1]);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = length2(dx, dy).max(1e-6);
    let mut worst = (0usize, 0.0f32);
    for (i, p) in points.iter().enumerate().take(points.len() - 1).skip(1) {
        let d = ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len;
        let dz = {
            let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / (len * len)).clamp(0.0, 1.0);
            (p.z - (a.z + (b.z - a.z) * t)).abs()
        };
        let d = d.max(dz);
        if d > worst.1 {
            worst = (i, d);
        }
    }
    if worst.1 <= tolerance {
        return vec![a, b];
    }
    let mut left = simplify(&points[..=worst.0], tolerance);
    let right = simplify(&points[worst.0..], tolerance);
    left.pop();
    left.extend(right);
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_corridor_finds_a_curb_near_ten_metres_before_the_sides(){
        struct Ahead;
        impl GroundProbe for Ahead {
            fn down(&mut self,_x:f32,y:f32,top:f32,bottom:f32)->Option<f32>{
                let z=if y>=9.5{0.15}else{0.};
                (top>=z && bottom<=z).then_some(z)
            }
        }
        let mut finder=EdgeFinder::new(EdgeSettings::default());
        for _ in 0..12 {finder.update_ahead(&mut Ahead,GtaVec::default(),GtaVec::new(0.,1.,0.));}
        assert!(finder.points().iter().any(|p|(p.position.y-9.5).abs()<0.01),
            "far end of the forward corridor was starved by the surrounding disk");
    }

    #[test]
    fn a_narrow_ledge_keeps_both_corners() {
        struct Narrow;
        impl GroundProbe for Narrow {
            fn down(&mut self,x:f32,_y:f32,top:f32,bottom:f32)->Option<f32>{
                let z=if (0. ..=0.2).contains(&x){0.4}else{0.};
                (top>=z && bottom<=z).then_some(z)
            }
        }
        let finder=EdgeFinder::new(EdgeSettings{spacing:0.1,..EdgeSettings::default()});
        for low in [(-1,0),(3,0)] {
            assert!(finder.refine(&mut Narrow,(1,0),low,0.4,0.,0.).is_some(),
                "a 20cm ledge top was rejected because its opposite edge drops away");
        }
    }

    #[test]
    fn a_curved_narrow_cap_is_sampled_along_its_length() {
        struct Ring;
        impl GroundProbe for Ring {
            fn down(&mut self,x:f32,y:f32,top:f32,bottom:f32)->Option<f32>{
                let r=(x*x+y*y).sqrt();
                let z=if (3.9..=4.1).contains(&r){0.4}else{0.};
                (top>=z && bottom<=z).then_some(z)
            }
            fn toward(&mut self,a:GtaVec,b:GtaVec)->Option<(GtaVec,GtaVec)>{
                let d=b.sub(a);let aa=d.x*d.x+d.y*d.y;
                if aa<1e-8{return None;}
                let bb=2.*(a.x*d.x+a.y*d.y);let mut hits=Vec::new();
                for r in [3.9f32,4.1] {
                    let cc=a.x*a.x+a.y*a.y-r*r;let disc=bb*bb-4.*aa*cc;
                    if disc<0.{continue;}
                    for t in [(-bb-disc.sqrt())/(2.*aa),(-bb+disc.sqrt())/(2.*aa)] {
                        let p=a.add(d.scale(t));
                        if (0. ..=1.).contains(&t) && (0. ..=0.4).contains(&p.z){
                            let sign=if r<4.{-1.}else{1.};
                            hits.push((t,p,GtaVec::new(sign*p.x/r,sign*p.y/r,0.)));
                        }
                    }
                }
                hits.into_iter().min_by(|a,b|a.0.total_cmp(&b.0)).map(|(_,p,n)|(p,n))
            }
        }
        let mut finder=EdgeFinder::new(EdgeSettings::default());
        for _ in 0..800 {finder.update(&mut Ring,GtaVec::default());}
        let lines=finder.polylines(GtaVec::default(),6.);
        let length:f32=lines.iter().flat_map(|l|l.windows(2)).map(|p|
            length2(p[1].x-p[0].x,p[1].y-p[0].y)).sum();
        assert!(length>35.,"curved cap became isolated short lines: length={length}, pieces={}",lines.len());
        assert!(lines.iter().flatten().all(|p|((p.x*p.x+p.y*p.y).sqrt()-4.).abs()<0.12));
    }

    #[test]
    fn curved_points_follow_the_edge_instead_of_chords_across_it() {
        let points:Vec<_>=(0..41).map(|i|{let a=-2.3+i as f32*4.6/40.;
            GtaVec::new(2.*a.cos(),2.*a.sin(),0.5)}).collect();
        let pieces=order_along(&points,0.5);
        assert_eq!(pieces.len(),1,"curved edge split or crossed itself");
        assert_eq!(pieces[0].len(),points.len());
        for pair in pieces[0].windows(2){assert!(length2(pair[0].x-pair[1].x,pair[0].y-pair[1].y)<0.24);}
    }

    #[test]
    fn neighbouring_parallel_edges_do_not_merge_into_a_zigzag() {
        let mut finder=EdgeFinder::new(EdgeSettings::default());
        for x in [0.,0.3] {for y in 0..9 {
            finder.points.push(EdgePoint{position:GtaVec::new(x,y as f32*0.5,0.5),drop:GtaVec::new(1.,0.,0.)});
        }}
        let lines=finder.polylines(GtaVec::default(),6.);
        assert_eq!(lines.len(),2,"parallel edges merged: {lines:?}");
        assert!(lines.iter().all(|line|line.iter().all(|p|(p.x-line[0].x).abs()<1e-5)));
    }

    /// A 15 cm curb along y = 3.3 (high side north), and a gentle ramp east.
    struct Street;
    impl GroundProbe for Street {
        fn down(&mut self, x: f32, y: f32, top: f32, bottom: f32) -> Option<f32> {
            let z = if y >= 3.3 && x < 4.0 { 0.15 } else if x >= 4.0 { (x - 4.0) * 0.3 } else { 0.0 };
            (top >= z && bottom <= z).then_some(z)
        }
    }

    #[test]
    fn a_curb_becomes_one_straight_line_and_a_slope_none() {
        let mut finder = EdgeFinder::new(EdgeSettings { rays: 100_000, ..EdgeSettings::default() });
        let center = GtaVec::new(0.0, 0.0, 0.0);
        assert!(finder.update(&mut Street, center));
        let lines = finder.polylines(center, 6.0);
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert_eq!(line.len(), 2, "straight curb simplifies to its ends: {line:?}");
        for p in line {
            assert!((p.y - 3.3).abs() < 0.01, "edge located to a centimetre: {p:?}");
            assert!((p.z - 0.15).abs() < 1e-4);
            assert!(p.x < 4.0);
        }
        assert!((line[0].x - line[1].x).abs() > 4.0, "runs along the curb: {line:?}");
        // The curb's east end, where it meets the ramp's foot, is a real step too.
        for p in finder.points().iter().filter(|p| p.position.x < 3.9) {
            assert!(p.drop.y < -0.9, "drops south: {p:?}");
        }
    }

    #[test]
    fn steep_faceted_bowl_surfaces_are_not_grind_edges() {
        struct Bowl;
        impl GroundProbe for Bowl {
            fn down(&mut self,x:f32,_y:f32,top:f32,bottom:f32)->Option<f32> {
                // Small collision seams across a steep sheet are not ledges.
                let z=1.5*x + (x/0.2).floor()*0.04;
                (top>=z && bottom<=z).then_some(z)
            }
        }
        let mut finder=EdgeFinder::new(EdgeSettings{rays:100_000,..EdgeSettings::default()});
        finder.update(&mut Bowl,GtaVec::default());
        assert!(finder.points().is_empty(),"steep bowl sheet acquired {} grind points",finder.points().len());
    }

    #[test]
    fn the_ray_budget_spreads_work_over_updates() {
        let mut finder = EdgeFinder::new(EdgeSettings::default());
        let center = GtaVec::new(0.0, 0.0, 0.0);
        let mut updates = 0;
        while finder.polylines(center, 6.0).is_empty() {
            finder.update(&mut Street, center);
            updates += 1;
            assert!(updates < 100);
        }
        assert!(updates > 3, "more than one update's worth of rays: {updates}");
    }
}
