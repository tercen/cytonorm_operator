//! cytonorm_operator — CytoNorm batch normalisation for Tercen.
//!
//! The projection follows the R `cytonorm_operator`, so a workflow can swap one for the other:
//! rows are channels, columns are cells, the **colour** factor carries the batch and the
//! **label** factor carries the type, where `Train` marks a batch control. The value is y.
//!
//! Two passes over the crosstab. The first reads the batch controls and fits one monotone
//! spline per batch, cluster and channel; the second applies it to every cell and writes the
//! result. Fitting reads a seeded subsample, as CytoNorm itself does, which keeps the memory
//! booking fixed no matter how large the study is.
pub mod context;
pub mod cytonorm;
pub mod input;
pub mod output;
pub mod pagecache;
pub mod progress;
pub mod props;
pub mod tson;
pub mod upload;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use tercen_rs::context::ContextBase;
use tercen_rs::{DevContext, TercenClient};

use cytonorm::Model;
use progress::Reporter;
use props::Settings;
use tson::TsonWriter;

const CHUNK: usize = 200_000;

pub fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

pub fn require_env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow!("{name} is not set"))
}

pub async fn run(task_id: &str) -> Result<()> {
    tracing::info!("cytonorm_operator starting (task_id={task_id})");
    let client = build_client().await?;
    let ctx = context::from_task_id(client, task_id).await?;
    execute(
        &ctx,
        Mode::Production {
            task_id: task_id.to_string(),
        },
    )
    .await
}

pub async fn run_dev(workflow_id: &str, step_id: &str) -> Result<()> {
    tracing::info!("cytonorm_operator starting in dev mode ({workflow_id} / {step_id})");
    let client = build_client().await?;
    let ctx = DevContext::from_workflow_step(client, workflow_id, step_id)
        .await
        .map_err(|e| anyhow!("load workflow {workflow_id} / step {step_id}: {e}"))?;
    execute(
        &ctx,
        Mode::Dev {
            workflow_id: workflow_id.to_string(),
            step_id: step_id.to_string(),
        },
    )
    .await
}

enum Mode {
    Production {
        task_id: String,
    },
    Dev {
        workflow_id: String,
        step_id: String,
    },
}

async fn build_client() -> Result<Arc<TercenClient>> {
    let client = TercenClient::from_env()
        .await
        .map_err(|e| anyhow!("connect to Tercen: {e}"))?;
    tracing::info!("connected to Tercen");
    Ok(Arc::new(client))
}

async fn execute(ctx: &ContextBase, mode: Mode) -> Result<()> {
    let t_start = Instant::now();
    tracing::info!(
        workflow = ctx.workflow_id(),
        step = ctx.step_id(),
        namespace = ctx.namespace(),
        "context loaded"
    );
    let rep = match &mode {
        Mode::Production { task_id } => Reporter::spawn(Arc::clone(ctx.client()), task_id.clone()),
        Mode::Dev { .. } => Reporter::silent(),
    };
    let s = props::settings_from_ctx(ctx)?;
    tracing::info!(?s, "properties");

    let n_cells = input::cell_count(ctx).await?;
    let layout = input::Layout::load(ctx, &s).await?;
    tracing::info!(
        n_cells,
        channels = layout.n_channels(),
        batches = layout.n_batches(),
        clusters = layout.n_clusters(),
        reference_columns = layout.n_reference_columns(),
        "projection"
    );

    rep.at(0, "Fitting the batch model");
    let t = Instant::now();
    let model = fit_model(ctx, &s, &layout, n_cells, &rep).await?;
    tracing::info!(
        secs = format!("{:.1}", t.elapsed().as_secs_f64()),
        fitted = model.fitted_keys().len(),
        "model fitted"
    );
    report_model(&rep, &layout, &model);

    let work_root = std::env::temp_dir().join(format!(
        "cytonorm_op_{}_{}",
        ctx.workflow_id(),
        ctx.step_id()
    ));
    std::fs::create_dir_all(&work_root)
        .with_context(|| format!("create {}", work_root.display()))?;
    let _guard = TempDirGuard(work_root.clone());
    let result_path = work_root.join("result.tson");
    {
        let f = std::fs::File::create(&result_path)
            .with_context(|| format!("create {}", result_path.display()))?;
        let w = std::io::BufWriter::with_capacity(4 << 20, pagecache::Releasing::new(f, 256 << 20));
        let mut w = TsonWriter::new(w)?;
        write_result(ctx, &s, &layout, &model, n_cells, &mut w, &rep, &work_root).await?;
    }
    let bytes = std::fs::metadata(&result_path)?.len();
    tracing::info!(bytes, "result written");
    pagecache::release_path(&result_path);

    rep.at(progress::UPLOAD.0, "Uploading the result");
    match mode {
        Mode::Production { task_id } => {
            upload::save_production(ctx, &task_id, &result_path, &rep).await?
        }
        Mode::Dev {
            workflow_id,
            step_id,
        } => {
            let saved = upload::save_dev(ctx, &workflow_id, &step_id, &result_path).await?;
            tracing::info!(task_id = saved.task_id, "dev result saved");
        }
    }
    rep.at(100, "Done");
    rep.info(format!(
        "CytoNorm complete: {n_cells} values in {:.1} s",
        t_start.elapsed().as_secs_f64()
    ));
    tracing::info!(
        total_secs = format!("{:.1}", t_start.elapsed().as_secs_f64()),
        peak_rss_kb = peak_rss_kb().unwrap_or(0),
        "done"
    );
    Ok(())
}

/// Pass one: read the batch controls, subsampled, and fit.
async fn fit_model(
    ctx: &ContextBase,
    s: &Settings,
    layout: &input::Layout,
    n_cells: usize,
    rep: &Reporter,
) -> Result<Model> {
    let mut sampler = input::ReferenceSampler::new(s, layout);
    let mut seen = 0usize;
    input::for_each_chunk(ctx, &[".ri", ".ci", ".y"], n_cells, CHUNK, |c| {
        sampler.offer(&c);
        seen += c.len();
        rep.at(
            progress::band(progress::READ, seen / 2, n_cells.max(1)),
            format!("Fitting: read {seen} of {n_cells}"),
        );
        Ok(())
    })
    .await?;
    let (groups, _n) = sampler.finish();
    if groups.is_empty() {
        anyhow::bail!(
            "no reference cells found: no column has '{}' in the type factor. Project the type \
             factor as a label, or set 'reference_value' to whatever marks a batch control.",
            s.reference_value
        );
    }
    Ok(Model::fit_groups(
        groups,
        layout.n_channels(),
        s.n_quantiles,
        s.min_cells,
    ))
}

/// Pass two: apply and write, in the shape asinh writes (value first, indices spilled).
#[allow(clippy::too_many_arguments)]
async fn write_result<W: std::io::Write>(
    ctx: &ContextBase,
    s: &Settings,
    layout: &input::Layout,
    model: &Model,
    n_cells: usize,
    w: &mut TsonWriter<W>,
    rep: &Reporter,
    work_root: &std::path::Path,
) -> Result<()> {
    let ns = output::value_column(ctx.namespace());
    let cols = output::result_columns(&ns);
    output::write_header(w, &table_name(ctx), n_cells, &cols, 1)?;

    let ri_path = work_root.join("ri.i32");
    let ci_path = work_root.join("ci.i32");
    let mut written = 0usize;
    {
        let mut ri_out = std::io::BufWriter::with_capacity(
            1 << 20,
            std::fs::File::create(&ri_path)
                .with_context(|| format!("create {}", ri_path.display()))?,
        );
        let mut ci_out = std::io::BufWriter::with_capacity(
            1 << 20,
            std::fs::File::create(&ci_path)
                .with_context(|| format!("create {}", ci_path.display()))?,
        );
        output::write_column_header(w, &cols[0], n_cells)?;
        w.f64_list_header(n_cells)?;
        input::for_each_chunk(ctx, &[".ri", ".ci", ".y"], n_cells, CHUNK, |mut c| {
            for k in 0..c.y.len() {
                let channel = c.ri[k].max(0) as usize;
                let key = layout.key_of_column(c.ci[k]);
                c.y[k] = model.apply_one(key, channel, c.y[k]);
            }
            w.f64_chunk(&c.y)?;
            output::write_i32_le(&mut ri_out, &c.ri)?;
            output::write_i32_le(&mut ci_out, &c.ci)?;
            written += c.len();
            rep.at(
                progress::band(progress::WRITE, written, n_cells.max(1)),
                format!("Normalising {written} of {n_cells}"),
            );
            Ok(())
        })
        .await?;
        use std::io::Write as _;
        ri_out.flush()?;
        ci_out.flush()?;
    }
    if written != n_cells {
        anyhow::bail!("the crosstab returned {written} cells where its schema says {n_cells}");
    }
    for (i, path) in [(1usize, &ri_path), (2usize, &ci_path)] {
        output::write_column_header(w, &cols[i], n_cells)?;
        w.i32_list_header(n_cells)?;
        output::pour(w, path, n_cells * 4)?;
        pagecache::release_path(path);
        let _ = std::fs::remove_file(path);
    }
    output::write_footer(w, false)?;
    let _ = s;
    Ok(())
}

fn report_model(rep: &Reporter, layout: &input::Layout, model: &Model) {
    let mut identity = 0;
    let mut fitted = 0;
    for key in model.fitted_keys() {
        for c in 0..model.n_channels {
            if model.is_identity(key, c) {
                identity += 1;
            } else {
                fitted += 1;
            }
        }
    }
    rep.info(format!(
        "CytoNorm: {} batches × {} cluster(s) × {} channels — {fitted} splines fitted, \
         {identity} left as identity",
        layout.n_batches(),
        layout.n_clusters(),
        model.n_channels
    ));
}

fn table_name(ctx: &ContextBase) -> String {
    format!("{}_{}", ctx.step_id(), ctx.qt_hash())
}

fn peak_rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    s.lines()
        .find(|l| l.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

struct TempDirGuard(PathBuf);
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub use cytonorm::Key as CytoKey;
