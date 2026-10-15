//! Immutable CAM jobs, independent of the desktop UI and document undo state.
use crate::{EngineError, Result, Session, cam};
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Default)]
pub struct Jobs {
    pub jobs: BTreeMap<u64, Arc<Job>>,
    pub next: u64,
}
pub struct Job {
    pub source: String,
    pub source_doc: Arc<solvecraft_doc::Document>,
    pub source_moves: BTreeMap<u64, solvecraft_doc::Mat>,
    pub input: cam::JobInput,
    pub cancelled: Arc<AtomicBool>,
    #[cfg(target_arch = "wasm32")]
    pending: Mutex<Option<std::pin::Pin<Box<dyn std::future::Future<Output = cam::Result<JobState>>>>>>,
    pub progress: Arc<AtomicUsize>,
    pub result: Mutex<JobState>,
}
#[derive(Clone)]
pub enum JobState {
    Running,
    Generated(Arc<cam::Toolpath>),
    Verifying(Arc<cam::Toolpath>),
    Verified(Arc<cam::Toolpath>, Arc<cam::verify::Report>),
    Failed(String),
    Cancelled,
}
impl Job {
    pub fn state(&self) -> Result<JobState> {
        self.result.lock().map(|s| s.clone()).map_err(|_| EngineError::Other("CAM worker state unavailable".into()))
    }
    pub fn set(&self, state: JobState) {
        if let Ok(mut s) = self.result.lock() {
            *s = state;
        }
    }
}
pub fn source(s: &Session) -> Result<String> {
    cam::fingerprint(&(&*s.doc, &s.pending_moves)).map_err(error)
}
pub fn error(e: cam::Error) -> EngineError {
    EngineError::Other(e.to_string())
}
pub fn input(s: &Session, program: u64) -> Result<cam::JobInput> {
    s.doc.manufacturing.validate().map_err(error)?;
    let m = &s.doc.manufacturing;
    let p = m.programs.iter().find(|p| p.id == program).ok_or_else(|| EngineError::Other("CAM program not found".into()))?;
    let setup = m.setups.iter().find(|x| x.id == p.setup).cloned().ok_or_else(|| EngineError::Other("CAM setup missing".into()))?;
    let mut operations = p
        .operations
        .iter()
        .map(|id| m.operations.iter().find(|o| o.id == *id).cloned().ok_or_else(|| EngineError::Other("CAM operation missing".into())))
        .collect::<Result<Vec<_>>>()?;
    for o in &mut operations {
        if let Some(source) = &o.source {
            let boundary = profile_boundary(s, &setup, source, o.tolerance)?;
            match &mut o.geometry {
                cam::Strategy::Contour { boundary: b, .. } | cam::Strategy::Pocket { boundary: b } => *b = boundary,
                _ => return Err(EngineError::Other("profile bound to unsupported operation".into())),
            }
        }
    }
    let tools = m.tools.iter().filter(|t| operations.iter().any(|o| o.tool == t.id)).cloned().collect::<Vec<_>>();
    if tools.iter().map(|t| t.number).collect::<std::collections::BTreeSet<_>>().len() != tools.len() {
        return Err(EngineError::Other("CAM tools must have unique tool numbers within a program".into()));
    }
    let mut fixtures = Vec::new();
    let mut fixture_groups = Vec::new();
    for reference in &setup.fixtures {
        let triangles = mesh(s, &setup, std::slice::from_ref(reference))?;
        fixture_groups.push(triangles.len());
        fixtures.extend(triangles);
    }
    let geometry = cam::Geometry { target: mesh(s, &setup, &setup.models)?, fixtures, fixture_groups };
    let mut input = cam::JobInput { setup, operations, tools, geometry, fingerprint: String::new() };
    input.fingerprint = cam::fingerprint(&input).map_err(error)?;
    Ok(input)
}
pub fn profile_boundary(s: &Session, setup: &cam::Setup, source: &cam::ProfileRef, tolerance: f64) -> Result<cam::Boundary> {
    let state = s.model.state();
    let sketch = state
        .sketches
        .iter()
        .find(|x| x.feature == source.sketch)
        .ok_or_else(|| EngineError::Other("CAM sketch missing; reselect the profile".into()))?;
    let mut wanted = source.curves.clone();
    wanted.sort();
    let candidates: Vec<_> = sketch
        .profiles
        .iter()
        .filter(|p| {
            let mut ids = p.curve_ids();
            ids.sort();
            ids == wanted
        })
        .collect();
    if candidates.len() != 1 {
        return Err(EngineError::Other("CAM profile missing or ambiguous; reselect it".into()));
    }
    let component = s.doc.feature(source.sketch).map(|f| f.component).ok_or_else(|| EngineError::Other("CAM sketch feature missing".into()))?;
    let transform = if source.occurrence.is_empty() {
        if component != 0 {
            return Err(EngineError::Other("CAM sketch needs an explicit occurrence path".into()));
        }
        solvecraft_doc::IDENTITY
    } else {
        let p = s.occurrence_placement(&source.occurrence)?;
        if p.component != component {
            return Err(EngineError::Other("CAM sketch occurrence mismatch".into()));
        }
        p.world
    };
    let convert = |ring: &crate::geom::Loop2| -> Result<Vec<[f64; 2]>> {
        let mut points = Vec::new();
        let mut z = None;
        for p in ring.polyline(cam::profile_tolerance(tolerance)) {
            let v = setup.frame.local(solvecraft_doc::apply_point(&transform, sketch.plane.to_world(p)));
            if z.is_some_and(|z: f64| (z - v.z).abs() > tolerance / 4.0) {
                return Err(EngineError::Other("CAM profile must be perpendicular to setup Z".into()));
            }
            z = Some(v.z);
            points.push([v.x, v.y]);
        }
        if points.first() == points.last() {
            points.pop();
        }
        Ok(points)
    };
    let p = candidates[0];
    Ok(cam::Boundary { outer: convert(&p.region.outer)?, islands: p.region.holes.iter().map(convert).collect::<Result<Vec<_>>>()? })
}
fn mesh(s: &Session, setup: &cam::Setup, refs: &[cam::BodyRef]) -> Result<Vec<[crate::geom::Vec3; 3]>> {
    let state = s.model.state();
    let mut triangles = Vec::new();
    for r in refs {
        let candidates: Vec<_> = state.bodies.iter().filter(|b| b.name == r.body).collect();
        if candidates.len() != 1 {
            return Err(EngineError::Other(format!("CAM body `{}` missing or ambiguous; reselect it", r.body)));
        }
        let b = candidates[0];
        let component = s.doc.model_body_component(b);
        let placement = if r.occurrence.is_empty() {
            if component != 0 {
                return Err(EngineError::Other("CAM component body requires an occurrence path".into()));
            }
            solvecraft_doc::IDENTITY
        } else {
            let p = s.occurrence_placement(&r.occurrence)?;
            if p.component != component {
                return Err(EngineError::Other("CAM body no longer belongs to selected occurrence".into()));
            }
            p.world
        };
        let transform = s.doc.model_body_offset(b).map(|offset| solvecraft_doc::mat_mul(&placement, offset)).unwrap_or(placement);
        let mesh = b.body.tessellate(cam::VERIFY_ERROR / 2.0)?;
        for indices in &mesh.triangles {
            let tri = mesh.tri(indices).ok_or_else(|| EngineError::Other("CAM mesh contains invalid indices".into()))?;
            triangles.push(tri.map(|v| setup.frame.local(solvecraft_doc::apply_point(&transform, v))));
            if triangles.len() > 200_000 {
                return Err(EngineError::Other("CAM mesh exceeds verification budget".into()));
            }
        }
    }
    Ok(triangles)
}
pub fn start(s: &mut Session, program: u64, background: bool) -> Result<u64> {
    if s.cam_jobs.jobs.len() >= 16 {
        let removable = s.cam_jobs.jobs.iter().find_map(|(id, j)| match j.state() {
            Ok(JobState::Running | JobState::Verifying(_)) => None,
            _ => Some(*id),
        });
        if let Some(id) = removable {
            s.cam_jobs.jobs.remove(&id);
        } else {
            return Err(EngineError::Other("too many running CAM jobs".into()));
        }
    }
    let input = input(s, program)?;
    let job = Arc::new(Job {
        source: source(s)?,
        source_doc: s.doc.clone(),
        source_moves: s.pending_moves.clone(),
        input,
        cancelled: Arc::new(AtomicBool::new(false)),
        #[cfg(target_arch = "wasm32")]
        pending: Mutex::new(None),
        progress: Arc::new(AtomicUsize::new(0)),
        result: Mutex::new(JobState::Running),
    });
    s.cam_jobs.next = s.cam_jobs.next.checked_add(1).ok_or_else(|| EngineError::Other("CAM job ID exhausted".into()))?;
    let id = s.cam_jobs.next;
    s.cam_jobs.jobs.insert(id, job.clone());
    dispatch(job, background, false)?;
    Ok(id)
}
pub fn dispatch(job: Arc<Job>, background: bool, verify: bool) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    if background {
        let failed = job.clone();
        if let Err(e) = std::thread::Builder::new().name("cam".into()).spawn(move || run(job, verify)) {
            failed.set(JobState::Failed(e.to_string()));
            return Err(EngineError::Other(e.to_string()));
        }
        return Ok(());
    }
    #[cfg(target_arch = "wasm32")]
    if background {
        let input = job.input.clone();
        let cancelled = job.cancelled.clone();
        let progress = job.progress.clone();
        let state = job.state()?;
        let future = async move {
            if verify {
                let path = match state {
                    JobState::Verifying(path) => path,
                    _ => return Err(cam::Error("generate first".into())),
                };
                let parsed = cam::nc::round_trip(&path, &input.setup, &input.tools)?;
                let report =
                    cam::verify::simulate_cooperative(&input, &parsed, Default::default(), || cancelled.load(Ordering::Relaxed), YieldOnce::default)
                        .await?;
                Ok(JobState::Verified(path, Arc::new(report)))
            } else {
                let mut generator = cam::generate::Generator::new(input)?;
                loop {
                    YieldOnce::default().await;
                    if cancelled.load(Ordering::Relaxed) {
                        return Ok(JobState::Cancelled);
                    }
                    let done = generator.step()?;
                    progress.store((generator.progress() * 100.0) as usize, Ordering::Relaxed);
                    if done {
                        break;
                    }
                }
                Ok(JobState::Generated(Arc::new(generator.finish()?)))
            }
        };
        *job.pending.lock().map_err(|_| EngineError::Other("CAM scheduler unavailable".into()))? = Some(Box::pin(future));
        return Ok(());
    }
    run(job, verify);
    Ok(())
}
fn run(job: Arc<Job>, verify: bool) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> cam::Result<JobState> {
        if verify {
            let state = job.state().map_err(|e| cam::Error(e.to_string()))?;
            let path = match state {
                JobState::Verifying(p) => p,
                _ => return Err(cam::Error("generate a path before simulation".into())),
            };
            let parsed = cam::nc::round_trip(&path, &job.input.setup, &job.input.tools)?;
            let report = cam::verify::simulate_cancellable(&job.input, &parsed, Default::default(), || job.cancelled.load(Ordering::Relaxed))?;
            Ok(JobState::Verified(path, Arc::new(report)))
        } else {
            let mut generator = cam::generate::Generator::new(job.input.clone())?;
            loop {
                if job.cancelled.load(Ordering::Relaxed) {
                    generator.cancel();
                    return Ok(JobState::Cancelled);
                }
                let done = generator.step()?;
                job.progress.store((generator.progress() * 100.0) as usize, Ordering::Relaxed);
                if done {
                    break;
                }
            }
            Ok(JobState::Generated(Arc::new(generator.finish()?)))
        }
    }));
    let state = if job.cancelled.load(Ordering::Relaxed) {
        JobState::Cancelled
    } else {
        match outcome {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => JobState::Failed(e.to_string()),
            Err(_) => JobState::Failed("CAM worker failed; no result published".into()),
        }
    };
    job.set(state);
}
pub fn job(s: &Session, id: u64) -> Result<Arc<Job>> {
    s.cam_jobs.jobs.get(&id).cloned().ok_or_else(|| EngineError::Other("CAM job missing; regenerate after reopening".into()))
}
pub fn current(s: &Session, job: &Job) -> Result<()> {
    if (!Arc::ptr_eq(&s.doc, &job.source_doc) && *s.doc != *job.source_doc) || s.pending_moves != job.source_moves {
        Err(EngineError::Other("CAM result is stale; regenerate after editing the design, tools or setup".into()))
    } else {
        Ok(())
    }
}

/// Resume each browser job once per UI frame or status query. No browser APIs in the engine.
#[cfg(target_arch = "wasm32")]
pub fn advance(job: &Job) {
    let Ok(mut pending) = job.pending.lock() else { return };
    if job.cancelled.load(Ordering::Relaxed) {
        *pending = None;
        job.set(JobState::Cancelled);
        return;
    }
    let Some(future) = pending.as_mut() else { return };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        future.as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    }));
    let state = match result {
        Ok(std::task::Poll::Pending) => return,
        Ok(std::task::Poll::Ready(Ok(state))) => state,
        Ok(std::task::Poll::Ready(Err(error))) => JobState::Failed(error.to_string()),
        Err(_) => JobState::Failed("CAM worker failed; no result published".into()),
    };
    *pending = None;
    job.progress.store(100, Ordering::Relaxed);
    job.set(state);
}
#[cfg(target_arch = "wasm32")]
#[derive(Default)]
struct YieldOnce(bool);
#[cfg(target_arch = "wasm32")]
impl std::future::Future for YieldOnce {
    type Output = ();
    fn poll(mut self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        if self.0 {
            std::task::Poll::Ready(())
        } else {
            self.0 = true;
            std::task::Poll::Pending
        }
    }
}
