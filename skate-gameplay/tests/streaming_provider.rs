//! Test the production provider without compiling missing upstream test modules.
#![allow(dead_code)]
#[path="../src/grind_world/octree.rs"] mod octree;
#[path="../src/grind_world/spline.rs"] mod spline;
#[path="../src/grind_world/provider.rs"] mod provider;

#[test]
fn streaming_preserves_contact_identity_and_clips_overlap(){
    use skate_data::skate_map::Rail;
    let rail=|end:f32|Rail{name:"stream".into(),closed:false,points:vec![[0.,0.,0.],[end,0.,0.]],native:None};
    let old=provider::StaticProvider::authored(&[rail(4.)]).unwrap();
    let new=old.extended(&[rail(10.)]).unwrap();
    assert_eq!(new.primitives().len(),2);
    assert_eq!(new.primitives()[0].start,old.primitives()[0].start);
    assert_eq!(new.primitives()[0].end,old.primitives()[0].end);
    assert_eq!(new.primitives()[0].owner,old.primitives()[0].owner);
    assert_eq!(new.metadata(0),old.metadata(0));
    assert_eq!(new.primitives()[1].start,new.primitives()[0].end);
    assert_eq!(new.primitives()[1].owner,old.primitives()[0].owner);
    assert_eq!(new.metadata(1).unwrap().spline_guids,old.metadata(0).unwrap().spline_guids);
    assert_eq!(new.extended(&[rail(10.)]).unwrap().primitives().len(),2);
    assert!(!new.query([5.,-1.,-1.],[9.,1.,1.]).unwrap().is_empty());
}
