//! Observed single-loop, single-circle profile references. Other profile schemas
//! fail explicitly; selection is never guessed from curve or object order.
use super::history::{empty, finish, payload, references, require_class, zeros};
use super::*;

#[derive(Debug, PartialEq)]
pub struct CircleProfile {
    pub id: u64,
    pub feature: u64,
    pub geometry: u64,
    pub circle: u64,
}

impl Design {
    pub fn circle_profiles(&self) -> Result<Vec<CircleProfile>> {
        if self.layout != 8 {
            return Err(invalid("profile decoding requires metadata layout 8"));
        }
        let Some(class) = self.classes.iter().find(|c| c.id == "4BD53E5A-0B3E-45E5-AE8B-02044306485A") else {
            return Ok(Vec::new());
        };
        if class.version != 0 {
            return Err(invalid("unsupported profile class version"));
        }
        let records = Records::new(self);
        let classes: BTreeMap<_, _> = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let circles = self.sketch_circles()?;
        let mut circle_ids = BTreeMap::new();
        for c in &circles {
            if let Some(identity) = c.profile_id
                && circle_ids.insert((c.sketch, identity), c.id).is_some()
            {
                return Err(invalid("ambiguous selected profile circle"));
            }
        }
        class
            .objects
            .iter()
            .map(|id| {
                let bytes = records.primary(*id)?;
                let mut r = payload(bytes, *id)?;
                empty(&mut r)?;
                zeros(&mut r, 2)?;
                let tracking = r.reference()?;
                require_class(&classes, tracking, "0D57BD2F-D09B-43FC-AD57-1E89A118C453", 1)?;
                if r.u32()? != 1 {
                    return Err(invalid("unsupported profile source count"));
                }
                r.wide_string()?; // persistent source identity; no inferred semantics
                let geometry = r.wide_string()?.parse::<u64>().map_err(|_| invalid("unsupported profile sketch identity"))?;
                require_class(&classes, geometry, "44A64366-4BD3-4B24-881A-F94C206E8F2D", 18)?;
                // The remaining presentation/cache fields are opaque, but the final
                // owning-feature reference is independently validated.
                let tail = bytes.len().checked_sub(11).filter(|at| *at >= r.at).ok_or_else(|| invalid("truncated profile tail"))?;
                let mut end = Reader { bytes: bytes.get(tail..).ok_or_else(|| invalid("profile tail"))?, at: 0 };
                let feature = end.reference()?;
                require_class(&classes, feature, "DD405BC2-D673-44F0-8833-5CB2A1C186C7", 8)?;
                let mut r = payload(records.primary(tracking)?, tracking)?;
                empty(&mut r)?;
                zeros(&mut r, 2)?;
                if r.reference()? != *id {
                    return Err(invalid("inconsistent profile tracking owner"));
                }
                for value in [1, 1, 1, 3] {
                    if r.u32()? != value {
                        return Err(invalid("unsupported profile topology (single-circle schema required)"));
                    }
                }
                let identity = (r.u64()?, r.u64()?);
                zeros(&mut r, 4)?;
                for value in [1, 1] {
                    if r.u32()? != value {
                        return Err(invalid("unsupported profile orientation"));
                    }
                }
                zeros(&mut r, 8)?;
                if r.u32()? != 1 {
                    return Err(invalid("unsupported profile flags"));
                }
                zeros(&mut r, 1)?;
                finish(&r)?;
                let circle = circle_ids.get(&(geometry, identity)).copied().ok_or_else(|| invalid("missing selected profile circle"))?;
                Ok(CircleProfile { id: *id, feature, geometry, circle })
            })
            .collect()
    }

    /// Resolve an extrusion's container through explicit profile references.
    pub fn selected_circle_profiles(&self, feature: u64, containers: &[u64]) -> Result<Vec<CircleProfile>> {
        let records = Records::new(self);
        let profiles: BTreeMap<_, _> = self.circle_profiles()?.into_iter().map(|p| (p.id, p)).collect();
        let classes: BTreeMap<_, _> = self.classes.iter().flat_map(|c| c.objects.iter().map(move |id| (*id, c))).collect();
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        for id in containers {
            require_class(&classes, *id, "2CA5A1CD-C99B-4C9B-91AF-57989148E841", 1)?;
            let bytes = records.primary(*id)?;
            let mut r = payload(bytes, *id)?;
            empty(&mut r)?;
            zeros(&mut r, 2)?;
            let ids = references(&mut r)?;
            let tail = bytes.len().checked_sub(11).filter(|at| *at >= r.at).ok_or_else(|| invalid("truncated profile container"))?;
            let mut r = Reader { bytes: bytes.get(tail..).ok_or_else(|| invalid("profile container tail"))?, at: 0 };
            if r.reference()? != feature {
                return Err(invalid("inconsistent profile container owner"));
            }
            for id in ids {
                if !seen.insert(id) {
                    return Err(invalid("duplicate selected profile"));
                }
                let p = profiles.get(&id).filter(|p| p.feature == feature).ok_or_else(|| invalid("missing selected profile or wrong owner"))?;
                selected.push(CircleProfile { id: p.id, feature: p.feature, geometry: p.geometry, circle: p.circle });
            }
        }
        Ok(selected)
    }
}
