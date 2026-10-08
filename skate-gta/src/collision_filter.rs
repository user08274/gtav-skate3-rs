//! Ignore small loose litter, without ignoring world curbs or long thin rails.
use crate::coords::GtaVec;
pub fn small_litter(min:GtaVec,max:GtaVec)->bool {
    let mut size=[(max.x-min.x).abs(),(max.y-min.y).abs(),(max.z-min.z).abs()];
    if size.iter().any(|v|!v.is_finite() || *v<=0.){return false;}
    size.sort_by(f32::total_cmp);
    size[2]<=0.65 && size[0]<=0.18
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn paper_and_bottles_are_filtered_but_rails_bins_and_posts_remain(){
        let origin=GtaVec::default();
        assert!(small_litter(origin,GtaVec::new(0.4,0.3,0.02)));
        assert!(small_litter(origin,GtaVec::new(0.08,0.08,0.3)));
        assert!(!small_litter(origin,GtaVec::new(4.,0.05,0.05)));
        assert!(!small_litter(origin,GtaVec::new(0.1,0.1,2.)));
        assert!(!small_litter(origin,GtaVec::new(0.5,0.5,1.)));
    }
}
