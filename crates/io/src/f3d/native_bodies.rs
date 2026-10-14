//! Bounded native geometry fallback via cadmpeg's Apache-2.0 codecs.
use super::*;
use cadmpeg_codec_f3d::F3dCodec;
use cadmpeg_codec_step::StepCodec;
use cadmpeg_ir::codec::write::{EncodeInput, Encoder, target::TargetRequest};
use cadmpeg_ir::codec::{Codec, DecodeOptions};
use solidcraft_doc::{Document, Model};

pub(super) fn recover(bytes: &[u8], name: &str) -> Result<(Document, Vec<String>)> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode(bytes, name))).map_err(|_| invalid("native body decoder stopped unexpectedly"))?
}

fn decode(bytes: &[u8], name: &str) -> Result<(Document, Vec<String>)> {
    let mut options = DecodeOptions::default();
    // Collection accounting counts SAB tokens as well as model entities. The
    // default 16M rejected the supplied Gridfinity archive before geometry decode.
    // Keep independent byte/work/depth ceilings; this is not an unlimited retry.
    options.policy.limits.max_collection_items = 64_000_000;
    options.policy.limits.max_input_bytes = MAX_F3D_BYTES as u64;
    options.policy.limits.max_decompressed_bytes_total = MAX_EXPANDED;
    options.policy.limits.max_retained_bytes = 2 << 30;
    let decoded = F3dCodec.decode(&mut Cursor::new(bytes), &options).map_err(|e| invalid(&format!("native bodies: {e}")))?;
    let mut warnings: Vec<_> = decoded.report().losses.iter().take(1000).map(|l| format!("Native decoder: {}", l.message)).collect();
    let plan = StepCodec::default()
        .plan(EncodeInput::new(decoded.ir(), None), TargetRequest::Explicit("ap214"))
        .map_err(|e| invalid(&format!("native body conversion: {e}")))?;
    warnings.extend(plan.report().losses.iter().take(1000).map(|l| format!("Body conversion: {}", l.message)));
    let mut step = Vec::new();
    plan.write_to(&mut step).map_err(|e| invalid(&format!("native body conversion: {e}")))?;
    let imported = crate::step_import_feature(&step, &format!("{name}.step"))?;
    warnings.extend(imported.warnings);
    let mut doc = Document::new(name);
    let id = doc.add_feature(imported.kind, Some("Recovered native bodies"))?;
    if let Some(feature) = doc.feature_mut(id) {
        feature.body_names = imported.body_names;
    }
    let mut model = Model::new();
    model.evaluate(&doc);
    if let Some(error) = model.results.iter().find_map(|r| r.error.as_ref()) {
        return Err(invalid(&format!("native body regeneration: {error}")));
    }
    if model.state().bodies.is_empty() {
        return Err(invalid("native decoder recovered no bodies"));
    }
    warnings.push("Recovered native bodies have no editable source history. Geometry conversion is incomplete and has not been verified against the original final shape.".into());
    Ok((doc, warnings))
}
