//! Request actual GTA shape-test hits on the script fiber, never model boxes.
use crate::{coords::{Frame,GtaVec},terrain::GroundProbe};
use skate_core::{math::Vector3,physics::{board_world::{ExternalQueries,ExternalLineHit,WorldLineHit},triangle_query::TriangleLineHit}};
use std::sync::mpsc::{self,Receiver,Sender};
use std::time::Duration;
enum Request {
    Line(Vector3,Vector3,f32,Sender<Option<ExternalLineHit>>),
    Nearby(Vector3,f32,Sender<Vec<[Vector3;3]>>),
}
pub struct NativeQueries {send:Sender<Request>,nearby_cache:std::sync::Mutex<std::collections::HashMap<[u32;4],Vec<[Vector3;3]>>>}
pub struct Service {frame:Frame,receive:Receiver<Request>}
pub fn channel(frame:Frame)->(NativeQueries,Service){
    let (send,receive)=mpsc::channel();
    (NativeQueries{send,nearby_cache:Default::default()},Service{frame,receive})
}
impl ExternalQueries for NativeQueries {
    fn line(&self,a:Vector3,b:Vector3,r:f32)->Option<ExternalLineHit>{
        let (send,receive)=mpsc::channel();
        self.send.send(Request::Line(a,b,r,send)).ok()?;
        receive.recv().ok().flatten()
    }
    fn nearby(&self,c:Vector3,r:f32)->Vec<[Vector3;3]>{
        let key=[c.x.to_bits(),c.y.to_bits(),c.z.to_bits(),r.to_bits()];
        if let Some(faces)=self.nearby_cache.lock().unwrap().get(&key){return faces.clone();}
        let (send,receive)=mpsc::channel();
        if self.send.send(Request::Nearby(c,r,send)).is_err(){return Vec::new();}
        let faces=receive.recv().unwrap_or_default();
        self.nearby_cache.lock().unwrap().insert(key,faces.clone());
        faces
    }
}
impl Service {
    pub fn pump(&self,probe:&mut dyn GroundProbe){
        let Ok(request)=self.receive.recv_timeout(Duration::from_millis(1)) else{return};
        match request {
            Request::Line(a,b,r,send)=>{let _=send.send(self.line(probe,a,b,r));}
            Request::Nearby(c,r,send)=>{let _=send.send(self.nearby(probe,c,r));}
        }
    }
    fn line(&self,probe:&mut dyn GroundProbe,start:Vector3,end:Vector3,radius:f32)->Option<ExternalLineHit>{
        let a=self.frame.to_gta(start);let b=self.frame.to_gta(end);let d=b.sub(a);
        let length2=d.x*d.x+d.y*d.y+d.z*d.z;
        if length2<1e-10{return None;}
        let along=d.normalized();
        let axis=if along.z.abs()<0.9{GtaVec::new(0.,0.,1.)}else{GtaVec::new(1.,0.,0.)};
        let cross=|a:GtaVec,b:GtaVec|GtaVec::new(a.y*b.z-a.z*b.y,a.z*b.x-a.x*b.z,a.x*b.y-a.y*b.x);
        let side=cross(along,axis).normalized();let up=cross(side,along).normalized();
        let offsets=if radius>0.{vec![GtaVec::default(),side.scale(radius),side.scale(-radius),up.scale(radius),up.scale(-radius)]}else{vec![GtaVec::default()]};
        let mut best:Option<(f32,GtaVec,GtaVec)>=None;
        for offset in offsets {
            if let Some((p,n))=probe.toward(a.add(offset),b.add(offset)) {
                let delta=p.sub(a.add(offset));
                let fraction=((delta.x*d.x+delta.y*d.y+delta.z*d.z)/length2).clamp(0.,1.);
                if best.is_none_or(|v|fraction<v.0){best=Some((fraction,p,n));}
            }
        }
        best.map(|(fraction,p,n)|ExternalLineHit{
            hit:WorldLineHit{geometry:TriangleLineHit{position:self.frame.to_skate(p),normal:self.frame.dir_to_skate(n),fraction,volume_parameter:[0.;3]},tag:0},
            surface:0,geometry_id:0,frame:[[1.,0.,0.,0.],[0.,1.,0.,0.],[0.,0.,1.,0.],[0.,0.,0.,1.]],
        })
    }
    fn nearby(&self,probe:&mut dyn GroundProbe,center:Vector3,radius:f32)->Vec<[Vector3;3]>{
        let center=self.frame.to_gta(center);let reach=radius.clamp(0.02,2.)+0.08;
        let mut triangles=Vec::new();let mut hits:Vec<(GtaVec,GtaVec)>=Vec::new();
        for direction in [GtaVec::new(1.,0.,0.),GtaVec::new(-1.,0.,0.),GtaVec::new(0.,1.,0.),GtaVec::new(0.,-1.,0.),GtaVec::new(0.,0.,1.),GtaVec::new(0.,0.,-1.)] {
            let outside=center.add(direction.scale(reach));
            // One diameter ray in each direction covers both halves of the
            // volume, including an inside start, without twelve half-rays.
            let hit=probe.toward(outside,center.sub(direction.scale(reach)));
            let Some((p,n))=hit else{continue};let n=n.normalized();
            if hits.iter().any(|(q,m)|{let d=p.sub(*q); d.x*d.x+d.y*d.y+d.z*d.z<0.0001 && m.x*n.x+m.y*n.y+m.z*n.z>0.98}){continue;}
            hits.push((p,n));
            let axis=if n.z.abs()<0.9{GtaVec::new(0.,0.,1.)}else{GtaVec::new(1.,0.,0.)};
            let cross=|a:GtaVec,b:GtaVec|GtaVec::new(a.y*b.z-a.z*b.y,a.z*b.x-a.x*b.z,a.x*b.y-a.y*b.x);
            let u=cross(axis,n).normalized().scale(0.04);let v=cross(n,u).normalized().scale(0.04);
            // A small local face at a real hit, with its native facing normal.
            // No extrusion, inferred height or empty interior bounding volume.
            for triangle in [[p.sub(u).sub(v),p.add(u).sub(v),p.add(u).add(v)],
                [p.sub(u).sub(v),p.add(u).add(v),p.sub(u).add(v)]] {
                triangles.push(triangle.map(|p|self.frame.to_skate(p)));
            }
        }
        triangles
    }
}

#[cfg(test)]mod tests {
    use super::*;
    struct Wall {owner:std::thread::ThreadId,calls:usize}
    impl GroundProbe for Wall {
        fn down(&mut self,_x:f32,_y:f32,_top:f32,_bottom:f32)->Option<f32>{None}
        fn toward(&mut self,a:GtaVec,b:GtaVec)->Option<(GtaVec,GtaVec)>{
            assert_eq!(std::thread::current().id(),self.owner);self.calls+=1;
            if b.x==a.x{return None;}let t=(1.-a.x)/(b.x-a.x);
            (0. ..=1.).contains(&t).then_some((a.lerp(b,t),GtaVec::new(-1.,0.,0.)))
        }
    }
    #[test]fn queries_are_serviced_on_the_caller_and_do_not_invent_boxes(){
        let mut probe=Wall{owner:std::thread::current().id(),calls:0};
        let (queries,service)=channel(Frame::new(GtaVec::default()));
        crate::bigstack::run_serviced(move||{
            let hit=queries.line(Vector3::ZERO,Vector3::new(2.,0.,0.),0.).unwrap();
            assert!((hit.hit.geometry.fraction-0.5).abs()<1e-5);
            assert!(queries.nearby(Vector3::ZERO,0.1).is_empty());
            let faces=queries.nearby(Vector3::new(0.95,0.,0.),0.1);
            assert!(!faces.is_empty());
            assert!(faces.iter().flatten().all(|p|(p.x-1.).abs()<1e-5));
            // Identical requests within one render frame reuse the exact
            // result, including misses, without more native ray calls.
            assert_eq!(queries.nearby(Vector3::new(0.95,0.,0.),0.1),faces);
            Ok(())
        },||service.pump(&mut probe)).unwrap();
        assert_eq!(probe.calls,13,"duplicate requests or missed half-rays added native work");
    }
}
