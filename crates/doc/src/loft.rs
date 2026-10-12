//! Loft sections retain the same certified face identity used by attached sketches.
use crate::{DocError, Document, LoftSection, ModelState, ProfileSel, Result};
use solvecraft_geom::{Plane, Region2};

impl LoftSection {
    pub(crate) fn region(&self, doc: &Document, state: &ModelState, author: u64) -> Result<(Plane, Region2)> {
        let (plane, regions) = if let Some(anchor) = &self.face {
            if self.sketch != 0 || self.point.is_some() || self.profiles != ProfileSel::All {
                return Err(DocError::Invalid("face loft section cannot also select a sketch or point".into()));
            }
            let plane = doc.resolve_face_anchor(anchor, author, state)?;
            let framed = crate::state_in_face_anchor_frame(doc, state, anchor, author)?;
            let projected = crate::project::resolve_anchored_face(doc, &framed, &plane, crate::sketch::LinkKind::Project, anchor)?;
            let mut sketch = crate::sketch::Sketch::new();
            sketch.add_link(crate::sketch::LinkKind::Project, projected.source, &projected.geom)?;
            let profiles = crate::sketch::find_profiles(&sketch);
            if profiles.iter().any(|p| !p.region.holes.is_empty()) {
                return Err(DocError::Invalid("loft face sections with holes are not supported yet".into()));
            }
            (plane, profiles.into_iter().map(|p| p.region).collect::<Vec<_>>())
        } else {
            let sketch =
                state.sketch(self.sketch).ok_or_else(|| DocError::Unknown(format!("loft sketch {} must precede its feature", self.sketch)))?;
            (sketch.plane, crate::select_profiles(sketch, &self.profiles)?)
        };
        let mut regions = crate::sketch::merge_regions(&regions);
        if regions.len() != 1 {
            return Err(DocError::Invalid("each loft section must be one connected profile".into()));
        }
        let region = regions.pop().ok_or_else(|| DocError::Invalid("empty loft section".into()))?;
        if !region.holes.is_empty() {
            return Err(DocError::Invalid("loft sections with holes are not supported yet".into()));
        }
        Ok((plane, region))
    }
}

pub(crate) fn validate_sections_json(value: &serde_json::Value) -> Result<()> {
    let Some(features) = value.get("features").and_then(serde_json::Value::as_array) else { return Ok(()) };
    for feature in features {
        if feature.get("type").and_then(serde_json::Value::as_str) != Some("loft") {
            continue;
        }
        let sections = feature
            .get("sections")
            .and_then(serde_json::Value::as_array)
            .filter(|s| (2..=50).contains(&s.len()))
            .ok_or_else(|| DocError::Invalid("loft needs 2..50 sections".into()))?;
        for section in sections {
            if let Some(face) = section.get("face").filter(|f| !f.is_null()) {
                crate::face_anchor::validate_face_anchor_json(face)?;
                if section.get("sketch").is_some_and(|v| v.as_u64() != Some(0))
                    || section.get("point").is_some_and(|v| !v.is_null())
                    || section.get("profiles").is_some_and(|v| v != &serde_json::json!({"type":"all"}))
                {
                    return Err(DocError::Invalid("ambiguous face loft section".into()));
                }
            } else if section.get("sketch").and_then(serde_json::Value::as_u64).is_none_or(|id| id == 0) || section.get("profiles").is_none() {
                return Err(DocError::Invalid("loft sketch section requires its sketch and profiles".into()));
            }
        }
    }
    Ok(())
}
