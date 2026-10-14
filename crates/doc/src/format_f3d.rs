//! Compatibility for explicitly identified, unpublished F3D development files.
//! Their independent numbering must never reinterpret a plain upstream format.
use crate::{DocError, Result};
use serde_json::Value;

fn version(v: &Value) -> Result<Option<u32>> {
    let Some(tag) = v.get("format").and_then(Value::as_str).and_then(|tag| tag.strip_prefix("solidcraft-f3d-dev/")) else {
        return Ok(None);
    };
    let version = tag
        .parse::<u32>()
        .ok()
        .filter(|version| (13..=30).contains(version))
        .ok_or_else(|| DocError::Invalid("unsupported F3D development format; supported tagged versions are 13 through 30".into()))?;
    Ok(Some(version))
}

pub(crate) fn upstream_version(v: &Value, ordinary: u32) -> Result<u32> {
    // These files predate the folded/pending mechanism. Later branch numbers
    // require the upstream upgrades after 22, not a future upstream version.
    Ok(version(v)?.map_or(ordinary, |version| version.min(22)))
}

pub(crate) fn migrate(v: &mut Value) -> Result<()> {
    let Some(version) = version(v)? else { return Ok(()) };
    if (15..=22).contains(&version) {
        crate::format::v13_to_v14(v)?;
    }
    for feature in v.get_mut("features").and_then(Value::as_array_mut).into_iter().flatten() {
        if version == 14 && feature.get("type").and_then(Value::as_str) == Some("sketch") {
            for link in feature.get_mut("sketch").and_then(|s| s.get_mut("links")).and_then(Value::as_array_mut).into_iter().flatten() {
                crate::format::mark_link_edges(link);
            }
        }
        // The development branch omitted this field when it meant uniform
        // spacing; upstream's omitted field means spacing by section distance.
        if feature.get("type").and_then(Value::as_str) == Some("loft")
            && let Some(feature) = feature.as_object_mut()
        {
            feature.entry("interpolation").or_insert_with(|| Value::String("uniform".into()));
        }
    }
    Ok(())
}
