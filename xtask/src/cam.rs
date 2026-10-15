//! CAM analytic validation and local-only Fusion oracle reports.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use solvecraft_cam::{self as cam, JobInput, nc, verify};
use std::path::{Path, PathBuf};

fn write(path: &Path, content: impl AsRef<[u8]>) -> Result<(), String> {
    std::fs::write(path, content).map_err(|e| format!("{}: {e}", path.display()))
}
fn json_file(path: &Path) -> Result<Value, String> {
    let bytes = read(path)?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > 128 * 1024 * 1024 {
        return Err("oracle artifact exceeds 128 MiB".into());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}
fn pretty(value: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}
fn directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())
}
fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn validate(root: &Path) -> Result<(), String> {
    let out = root.join("target/cam-validation");
    directory(&out)?;
    let mut cases = Vec::new();
    for name in ["face", "contour", "pocket", "drill"] {
        let result = (|| -> Result<Value, String> {
            let input = cam::corpus::fixture(name).map_err(|e| e.to_string())?;
            let path = cam::generate::generate(input.clone()).map_err(|e| e.to_string())?;
            let parsed = nc::round_trip(&path, &input.setup, &input.tools).map_err(|e| e.to_string())?;
            let report = verify::simulate(&input, &parsed, Default::default()).map_err(|e| e.to_string())?;
            let expected = if name == "contour" { 6.0 } else { 5.0 };
            let actual = verify::probe_height(&input, &parsed, [10.0, 10.0]).map_err(|e| e.to_string())?;
            let dimensions_required = matches!(name, "face" | "drill");
            let pass = report.status == verify::Status::Pass
                && (actual - expected).abs() <= cam::TOLERANCE
                && (!dimensions_required || report.finished_dimensions == verify::Status::Pass);
            let dir = out.join(name);
            directory(&dir)?;
            write(&dir.join("input.json"), pretty(&json!(input))?)?;
            write(&dir.join("toolpath.json"), pretty(&json!(path))?)?;
            let code = nc::post(&path, &input.setup).map_err(|e| e.to_string())?;
            write(&dir.join("program.nc"), &code)?;
            write(&dir.join("solve.html"), program_html(&code, &report))?;
            write(&dir.join("stock-difference.svg"), stock_overlay(&input, &report))?;
            write(&dir.join("validation.json"), pretty(&json!(report))?)?;
            write(&dir.join("stock.stl"), verify::stock_stl(&report, input.setup.stock.min.z).map_err(|e| e.to_string())?)?;
            write(&dir.join("paths.svg"), overlay(&input, &parsed, None)?)?;
            Ok(
                json!({"case":name,"pass":pass,"status":report.status,"dimensional_certification_required":dimensions_required,"finished_dimensions":report.finished_dimensions,"probe":{"expected":expected,"actual":actual},"cells":report.cells.len(),"work":report.work,"findings":report.findings.iter().take(5).collect::<Vec<_>>() }),
            )
        })();
        cases.push(result.unwrap_or_else(|e| json!({"case":name,"pass":false,"error":e})));
    }
    report(&out, &cases, "Analytic CAM validation")?;
    println!("CAM validation: {}/{} passed → {}", cases.iter().filter(|v| v["pass"] == true).count(), cases.len(), out.display());
    if cases.iter().all(|v| v["pass"] == true) { Ok(()) } else { Err("CAM analytic validation failed; see report.json".into()) }
}
pub fn oracle(root: &Path, args: &[&str]) -> Result<(), String> {
    let dir = args.windows(2).find(|w| w[0] == "--dir").map(|w| PathBuf::from(w[1])).ok_or("cam-oracle requires --dir LOCAL_ORACLE_DIRECTORY")?;
    let canonical = std::fs::canonicalize(&dir).map_err(|e| e.to_string())?;
    if canonical.starts_with(root) {
        let ignored = std::process::Command::new("git")
            .current_dir(root)
            .args(["check-ignore", "--quiet", "--"])
            .arg(&canonical)
            .status()
            .map_err(|e| e.to_string())?;
        if !ignored.success() {
            return Err("Fusion storage inside this repository must be git-ignored (use plan/fusion/cam)".into());
        }
    }
    let out = dir.join("comparison");
    directory(&out)?;
    let mut cases = Vec::new();
    for name in ["face", "contour", "pocket", "drill"] {
        let case = dir.join(name);
        let value = compare_case(&case, &out.join(name)).unwrap_or_else(|e| json!({"pass":false,"status":"incomplete","error":e}));
        let mut value = value;
        value["case"] = json!(name);
        cases.push(value);
    }
    report(&out, &cases, "Local Fusion CAM comparison")?;
    let _ = root;
    println!("Fusion CAM oracle: {}/{} passed → {}", cases.iter().filter(|v| v["pass"] == true).count(), cases.len(), out.display());
    if cases.iter().all(|v| v["pass"] == true) {
        Ok(())
    } else {
        Err("Fusion parity unverified: missing, failed or inconclusive reference cases".into())
    }
}
fn compare_case(dir: &Path, out: &Path) -> Result<Value, String> {
    let manifest = json_file(&dir.join("manifest.json"))?;
    for field in ["fusion_version", "post_sha256", "captured_at", "input_fingerprint"] {
        if manifest[field].as_str().is_none_or(str::is_empty) {
            return Err(format!("missing reference provenance: {field}"));
        }
    }
    let digest = format!("{:x}", Sha256::digest(read(&dir.join("post.cps"))?));
    if manifest["post_sha256"] != digest {
        return Err("reference post hash mismatch".into());
    }
    if manifest["capture_complete"] != true {
        return Err("reference capture is incomplete".into());
    }
    let input_value = json_file(&dir.join("input.json"))?;
    let reference_value = json_file(&dir.join("reference-input.json"))?;
    if input_value != reference_value {
        return Err("SolveCraft and reference inputs differ; comparison invalid".into());
    }
    let input: JobInput = serde_json::from_value(input_value).map_err(|e| e.to_string())?;
    if manifest["input_fingerprint"] != input.fingerprint {
        return Err("manifest input fingerprint mismatch".into());
    }
    let mut unhashed = input.clone();
    unhashed.fingerprint.clear();
    if cam::fingerprint(&unhashed).map_err(|e| e.to_string())? != input.fingerprint {
        return Err("input fingerprint does not describe supplied geometry and settings".into());
    }
    for name in ["setup-sheet.html", "stock.stl", "simulation.json", "post-properties.json"] {
        if read(&dir.join(name))?.is_empty() {
            return Err(format!("empty artifact {name}"));
        }
    }
    let simulation = json_file(&dir.join("simulation.json"))?;
    if simulation["status"] != "pass" {
        return Err("reference simulation not passing".into());
    }
    let reference = String::from_utf8(read(&dir.join("reference.nc"))?).map_err(|e| e.to_string())?;
    let reference_code = reference;
    let reference = nc::parse(&reference_code, &input.tools).map_err(|e| e.to_string())?;
    let path = cam::generate::generate(input.clone()).map_err(|e| e.to_string())?;
    let own = nc::round_trip(&path, &input.setup, &input.tools).map_err(|e| e.to_string())?;
    let ours = verify::simulate(&input, &own, Default::default()).map_err(|e| e.to_string())?;
    let theirs = verify::simulate(&input, &reference, Default::default()).map_err(|e| e.to_string())?;
    directory(out)?;
    write(&out.join("solve.html"), program_html(&nc::post(&path, &input.setup).map_err(|e| e.to_string())?, &ours))?;
    write(&out.join("fusion.html"), program_html(&reference_code, &theirs))?;
    write(&out.join("stock-difference.svg"), stock_overlay(&input, &ours))?;
    write(&out.join("solve-validation.json"), pretty(&json!(ours))?)?;
    write(&out.join("fusion-nc-validation.json"), pretty(&json!(theirs))?)?;
    write(&out.join("paths.svg"), overlay(&input, &own, Some(&reference))?)?;
    let mesh = cam::compare::read_stl(&read(&dir.join("stock.stl"))?).map_err(|e| e.to_string())?;
    let stock = cam::compare::stock_reports(&ours, &theirs, cam::TOLERANCE).map_err(|e| e.to_string())?;
    let mesh_check = cam::compare::stock_mesh(&ours, &mesh, input.setup.stock.min.z, cam::TOLERANCE).map_err(|e| e.to_string())?;
    let metadata = json_file(&dir.join("setup-sheet-values.json"))?;
    let setup_values_match =
        metadata["setup"] == json!(input.setup) && metadata["tools"] == json!(input.tools) && metadata["operations"] == json!(input.operations);
    let pass = ours.status == verify::Status::Pass
        && theirs.status == verify::Status::Pass
        && ours.finished_dimensions == verify::Status::Pass
        && theirs.finished_dimensions == verify::Status::Pass
        && stock.status == verify::Status::Pass
        && mesh_check.status == verify::Status::Pass
        && setup_values_match;
    Ok(
        json!({"pass":pass,"status":if pass{"pass"}else{"inconclusive"},"solve":ours.status,"reference":theirs.status,"finished_dimensions":[ours.finished_dimensions,theirs.finished_dimensions],"stock":stock,"reference_stl":mesh_check,"setup_values_match":setup_values_match,"distance_delta":ours.cutting_distance-theirs.cutting_distance,"minutes_delta":ours.cutting_minutes-theirs.cutting_minutes,"volume_intervals":[[ours.remaining_volume_low,ours.remaining_volume_high],[theirs.remaining_volume_low,theirs.remaining_volume_high]]}),
    )
}
fn overlay(input: &JobInput, a: &nc::Parsed, b: Option<&nc::Parsed>) -> Result<String, String> {
    let min = input.setup.stock.min;
    let max = input.setup.stock.max;
    let w = (max.x - min.x).max(1.0);
    let h = (max.y - min.y).max(1.0);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#101820\"/>",
        min.x - 5.0,
        min.y - 5.0,
        w + 10.0,
        h + 10.0,
        min.x - 5.0,
        min.y - 5.0,
        w + 10.0,
        h + 10.0
    );
    for (program, color, name) in std::iter::once((a, "#42c4ff", "solve")).chain(b.map(|b| (b, "#ffb347", "fusion"))) {
        for s in nc::segments(program, &input.setup).map_err(|e| e.to_string())? {
            svg.push_str(&format!(
                "<a href=\"{name}.html#L{line}\"><path d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" opacity=\"{}\"><title>NC line {}</title></path></a>",
                s.from.x,
                s.from.y,
                s.to.x,
                s.to.y,
                color,
                w / 400.0,
                if s.rapid { 0.25 } else { 0.8 },
                s.line,
                line = s.line
            ));
        }
    }
    svg.push_str("</svg>");
    Ok(svg)
}
fn report(out: &Path, cases: &[Value], title: &str) -> Result<(), String> {
    write(&out.join("report.json"), pretty(&json!({"version":cam::VERSION,"cases":cases}))?)?;
    let mut html = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title><style>body{{font:16px system-ui;margin:40px;max-width:1100px}}pre{{white-space:pre-wrap;background:#eee;padding:16px}}img{{width:480px}}a{{color:#145aa8}}</style><h1>{title}</h1><p>Blue: SolveCraft; orange: Fusion. Path and time differences are diagnostic, not equivalence gates.</p>"
    );
    for c in cases {
        let name = c["case"].as_str().unwrap_or("unknown");
        html.push_str(&format!(
            "<h2>{}</h2><pre>{}</pre><a href=\"{}/paths.svg\">Toolpath overlay</a><br><img src=\"{}/paths.svg\" alt=\"Toolpath overlay\">",
            esc(name),
            esc(&pretty(c)?),
            esc(name),
            esc(name)
        ));
        html.push_str(&format!("<p><a href=\"{name}/solve.html\">SolveCraft line-linked diagnostics</a></p><img src=\"{name}/stock-difference.svg\" alt=\"Remaining material relative to protected geometry\">"));
    }
    write(&out.join("report.html"), html)
}

fn program_html(code: &str, validation: &verify::Report) -> String {
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>NC diagnostics</title><style>body{font:16px system-ui;margin:30px}pre{line-height:1.6}pre a{display:block;color:inherit;text-decoration:none}a:target{background:#ffc}code{white-space:pre}</style><h1>NC diagnostics</h1><ul>",
    );
    for f in &validation.findings {
        let label = esc(&format!("{}: {}", f.kind, f.detail));
        if let Some(line) = f.line {
            html.push_str(&format!("<li><a href=\"#L{line}\">Line {line}: {label}</a></li>"));
        } else {
            html.push_str(&format!("<li>{label}</li>"));
        }
    }
    html.push_str("</ul><pre>");
    for (i, line) in code.lines().enumerate() {
        let n = i + 1;
        html.push_str(&format!("<a id=\"L{n}\" href=\"#L{n}\">{n:6}  <code>{}</code></a>", esc(line)));
    }
    html.push_str("</pre>");
    html
}
fn stock_overlay(input: &JobInput, validation: &verify::Report) -> String {
    let min = input.setup.stock.min;
    let max = input.setup.stock.max;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\"><title>Sampled stock height above protected geometry; diagnostic only. Orange: remaining stock; red: definite gouge; blue: within tolerance.</title>",
        min.x,
        min.y,
        max.x - min.x,
        max.y - min.y
    );
    let step = validation.cells.len().div_ceil(20_000).max(1);
    for c in validation.cells.iter().step_by(step) {
        let excess = c.low - c.target_high;
        let gouge = c.target_low - c.high;
        let color = if gouge > validation.tolerance {
            "#e33"
        } else if excess > validation.tolerance {
            "#fa3"
        } else {
            "#29a6d2"
        };
        svg.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{color}\"><title>Height [{:.6}, {:.6}] mm; excess at least {:.6} mm</title></rect>",c.min[0],c.min[1],c.max[0]-c.min[0],c.max[1]-c.min[1],c.low,c.high,excess));
    }
    svg.push_str("</svg>");
    svg
}
