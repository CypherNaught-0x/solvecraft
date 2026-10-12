//! Bounded, persistent references to directed analytic profile pieces.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{DocError, PieceProfileBoundary, ProfileSel, Result, SolvedSketch};

fn invalid() -> DocError {
    DocError::Invalid("invalid analytic profile piece reference".into())
}

fn keys(value: &Value, expected: &[&str]) -> Result<()> {
    let fields = value.as_object().ok_or_else(invalid)?;
    if fields.len() != expected.len() || fields.keys().any(|key| !expected.contains(&key.as_str())) {
        return Err(invalid());
    }
    Ok(())
}

/// Check raw budgets before copying into typed document structures.
pub(crate) fn validate_raw(value: &Value) -> Result<()> {
    if let Some(seams) = value.get("circle_seams") {
        keys(value, &["type", "profiles", "circle_seams"])?;
        let seams = seams.as_object().ok_or_else(invalid)?;
        if seams.len() > 20_000
            || seams.iter().any(|(id, angle)| {
                id.is_empty()
                    || id.len() > 512
                    || id.chars().any(char::is_control)
                    || angle.as_f64().is_none_or(|angle| !angle.is_finite() || !(0.0..std::f64::consts::TAU).contains(&angle))
            })
        {
            return Err(invalid());
        }
    } else {
        keys(value, &["type", "profiles"])?;
    }
    if value.get("type").and_then(Value::as_str) != Some("piece_boundaries") {
        return Err(invalid());
    }
    let profiles = value.get("profiles").and_then(Value::as_array).ok_or_else(invalid)?;
    if profiles.is_empty() || profiles.len() > 128 {
        return Err(invalid());
    }
    let mut total = 0usize;
    for profile in profiles {
        keys(profile, &["outer", "holes"])?;
        let holes = profile.get("holes").and_then(Value::as_array).ok_or_else(invalid)?;
        if holes.len() > 128 {
            return Err(invalid());
        }
        for boundary in std::iter::once(profile.get("outer").ok_or_else(invalid)?).chain(holes) {
            let pieces = boundary.as_array().ok_or_else(invalid)?;
            total += pieces.len();
            if pieces.is_empty() || pieces.len() > 512 || total > 20_000 {
                return Err(invalid());
            }
            for piece in pieces {
                keys(piece, &["curve", "piece", "count", "reversed"])?;
                let id = piece.get("curve").and_then(Value::as_str).ok_or_else(invalid)?;
                let index = piece.get("piece").and_then(Value::as_u64).ok_or_else(invalid)?;
                let count = piece.get("count").and_then(Value::as_u64).ok_or_else(invalid)?;
                if id.is_empty()
                    || id.len() > 512
                    || id.chars().any(char::is_control)
                    || index == 0
                    || index > count
                    || count > 50_000
                    || piece.get("reversed").and_then(Value::as_bool).is_none()
                {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

pub fn piece_profile_selection_from_value(value: &Value) -> Result<ProfileSel> {
    validate_raw(value)?;
    let selection: ProfileSel = serde_json::from_value(value.clone()).map_err(|_| invalid())?;
    if let ProfileSel::PieceBoundaries { profiles, circle_seams } = &selection {
        validate(profiles)?;
        validate_seams(profiles, circle_seams)?;
    }
    Ok(selection)
}

pub(crate) fn validate(profiles: &[PieceProfileBoundary]) -> Result<()> {
    if profiles.is_empty() || profiles.len() > 128 {
        return Err(invalid());
    }
    let mut total = 0usize;
    let mut identities = BTreeSet::new();
    let mut counts = BTreeMap::new();
    for profile in profiles {
        if profile.holes.len() > 128
            || !identities.insert((&profile.outer, &profile.holes))
            || profile.holes.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        for boundary in std::iter::once(&profile.outer).chain(&profile.holes) {
            total += boundary.len();
            if boundary.is_empty() || boundary.len() > 512 || total > 20_000 || boundary.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(invalid());
            }
            for part in boundary {
                if part.curve.is_empty()
                    || part.curve.len() > 512
                    || part.curve.chars().any(char::is_control)
                    || part.piece == 0
                    || part.piece > part.count
                    || part.count > 50_000
                    || !seen.insert((&part.curve, part.piece))
                    || counts.insert(&part.curve, part.count).is_some_and(|old| old != part.count)
                {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_seams(profiles: &[PieceProfileBoundary], seams: &BTreeMap<String, f64>) -> Result<()> {
    if seams.len() > 20_000 {
        return Err(invalid());
    }
    let sources = profiles
        .iter()
        .flat_map(|profile| std::iter::once(&profile.outer).chain(&profile.holes))
        .flatten()
        .map(|part| &part.curve)
        .collect::<BTreeSet<_>>();
    for (id, angle) in seams {
        if !sources.contains(id) || !angle.is_finite() || !(0.0..std::f64::consts::TAU).contains(angle) {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(crate) fn indices(ss: &SolvedSketch, profiles: &[PieceProfileBoundary], seams: &BTreeMap<String, f64>) -> Result<Vec<usize>> {
    validate(profiles)?;
    validate_seams(profiles, seams)?;
    if !seams.is_empty() {
        let circles: BTreeSet<_> =
            ss.sketch.curves.iter().filter(|curve| matches!(curve.kind, crate::sketch::CurveKind::Circle { .. })).map(|curve| &curve.id).collect();
        if seams.keys().any(|id| !circles.contains(id)) {
            return Err(invalid());
        }
    }
    let current = ss
        .profiles
        .iter()
        .map(|profile| profile.piece_boundaries_with_seams(seams).map(|(outer, holes)| PieceProfileBoundary { outer, holes }))
        .collect::<Vec<_>>();
    profiles
        .iter()
        .map(|identity| {
            let mut matches = current.iter().enumerate().filter(|(_, candidate)| candidate.as_ref() == Some(identity));
            let (index, _) = matches.next().ok_or_else(|| DocError::Unknown(format!("analytic profile pieces changed or missing in {}", ss.name)))?;
            if matches.next().is_some() {
                return Err(DocError::Invalid(format!("ambiguous analytic profile pieces in {}", ss.name)));
            }
            Ok(index)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use serde_json::json;
    use solidcraft_geom::Vec2;

    #[test]
    fn piece_references_are_bounded_canonical_and_disjoint() {
        let part = json!({"curve":"circle","piece":1,"count":2,"reversed":false});
        let good = json!({"type":"piece_boundaries","profiles":[{"outer":[part],"holes":[]}]});
        assert!(piece_profile_selection_from_value(&good).is_ok());
        for (key, bad) in [
            ("piece", json!(0)),
            ("piece", json!(3)),
            ("count", json!(50_001)),
            ("count", json!(-1)),
            ("reversed", json!(0)),
            ("curve", json!("x".repeat(513))),
            ("curve", json!("x\ny")),
        ] {
            let mut value = good.clone();
            value["profiles"][0]["outer"][0][key] = bad;
            assert!(piece_profile_selection_from_value(&value).is_err(), "{key}");
        }
        for bad in [
            json!({"outer":[part,part],"holes":[]}),
            json!({"outer":[part],"holes":[[part]]}),
            json!({"outer":[],"holes":[]}),
            json!({"outer":[part],"holes":[],"extra":true}),
        ] {
            let value = json!({"type":"piece_boundaries","profiles":[bad]});
            assert!(piece_profile_selection_from_value(&value).is_err());
        }
        let mut bad = good.clone();
        bad["profiles"] = json!(vec![good["profiles"][0].clone(); 129]);
        assert!(piece_profile_selection_from_value(&bad).is_err());
        let mut bad = good.clone();
        bad["profiles"][0]["outer"] = json!(vec![part; 513]);
        assert!(piece_profile_selection_from_value(&bad).is_err());
    }

    #[test]
    fn saved_circle_seams_are_optional_bounded_and_referenced() {
        let good = json!({"type":"piece_boundaries","profiles":[{"outer":[{"curve":"circle","piece":1,"count":4,"reversed":false}],"holes":[]}]});
        let legacy = piece_profile_selection_from_value(&good).unwrap();
        assert_eq!(serde_json::to_value(legacy).unwrap(), good);
        for seams in [json!({"circle":0.0}), json!({"circle":std::f64::consts::PI})] {
            let mut value = good.clone();
            value["circle_seams"] = seams;
            let parsed = piece_profile_selection_from_value(&value).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), value);
        }
        for seams in [json!([]), json!({"missing":1.0}), json!({"circle":-0.1}), json!({"circle":std::f64::consts::TAU}), json!({"circle":"1"})] {
            let mut value = good.clone();
            value["circle_seams"] = seams;
            assert!(piece_profile_selection_from_value(&value).is_err(), "{value}");
        }
        let ProfileSel::PieceBoundaries { profiles, .. } = piece_profile_selection_from_value(&good).unwrap() else { panic!("selection") };
        for angle in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(validate_seams(&profiles, &BTreeMap::from([("circle".into(), angle)])).is_err());
        }
    }

    #[test]
    fn analytic_piece_identity_includes_holes_and_never_trusts_cached_profiles() {
        let mut sketch = crate::sketch::Sketch::new();
        sketch.add_circle(Vec2::ZERO, 30., None, Some("left")).unwrap();
        sketch.add_circle(Vec2::new(40., 0.), 30., None, Some("right")).unwrap();
        sketch.add_circle(Vec2::new(20., 0.), 2., None, Some("hole")).unwrap();
        let profiles = crate::sketch::find_profiles(&sketch);
        let ring = profiles.iter().find(|p| !p.hole_curves.is_empty()).unwrap();
        let (outer, holes) = ring.piece_boundaries().unwrap();
        assert_eq!(
            outer.iter().map(|p| (p.curve.as_str(), p.piece, p.count, p.reversed)).collect::<Vec<_>>(),
            [("left", 1, 2, false), ("right", 2, 2, false)]
        );
        assert_eq!(holes.len(), 1);
        assert!(holes[0][0].reversed);
        let copy: crate::sketch::Profile = serde_json::from_str(&serde_json::to_string(ring).unwrap()).unwrap();
        assert!(copy.piece_boundaries().is_none());
        let identity = PieceProfileBoundary { outer, holes };
        validate(std::slice::from_ref(&identity)).unwrap();
        sketch.add_circle(Vec2::new(20., 5.), 1., None, Some("new-hole")).unwrap();
        assert!(
            !crate::sketch::find_profiles(&sketch).iter().any(|p| p.piece_boundaries() == Some((identity.outer.clone(), identity.holes.clone())))
        );
    }
}
