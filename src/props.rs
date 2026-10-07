//! Operator properties.
//!
//! Names follow the R `cytonorm_operator` where they overlap (`cluster`, `number_of_cells`), so
//! a user swapping one for the other finds what they expect.
use anyhow::{Result, bail};
use tercen_rs::PropertyReader;
use tercen_rs::context::ContextBase;

#[derive(Debug, Clone)]
pub struct Settings {
    /// Number of FlowSOM clusters. **This operator does not cluster**: clustering is its own
    /// step, `flowsom_rust_operator` (or the R `flowsom_operator`), whose labels arrive here
    /// through `cluster_factor`. Above 1 without such a factor is refused rather than ignored.
    /// A single cluster is CytoNorm without clustering, which is a legitimate configuration.
    pub cluster: usize,
    /// Reference cells sampled per batch and cluster to fit the quantiles. CytoNorm subsamples
    /// for training; this is the same idea and it is what keeps the booking fixed.
    pub number_of_cells: usize,
    /// Seed for that subsample, so a fit is repeatable.
    pub seed: u64,
    /// Quantiles. 99 is what runs 7–9 use; cytonormpy's notebook default is 101.
    pub n_quantiles: usize,
    /// Reference cells a (batch, cluster) needs before it is fitted at all.
    pub min_cells: usize,
    /// Column factor naming the batch. Empty uses the first colour factor, as the R operator does.
    pub batch_factor: String,
    /// Column factor naming the reference/sample type. Empty uses the first label factor.
    pub type_factor: String,
    /// Value in the type factor that marks a batch control. The R operator uses `Train`.
    pub reference_value: String,
    /// Column factor holding a precomputed cluster label per cell. Empty means one cluster.
    pub cluster_factor: String,
    pub collect_max_cells: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cluster: 1,
            number_of_cells: 6000,
            seed: 1,
            n_quantiles: 99,
            min_cells: 101,
            batch_factor: String::new(),
            type_factor: String::new(),
            reference_value: "Train".into(),
            cluster_factor: String::new(),
            collect_max_cells: crate::output::COLLECT_MAX_CELLS,
        }
    }
}

pub fn settings_from_ctx(ctx: &ContextBase) -> Result<Settings> {
    let pr = PropertyReader::from_operator_settings(ctx.operator_settings());
    // Every numeric property is parsed as f64 and cast: Tercen stores "6000.0" for a property an
    // operator reads as an integer, and a failed integer parse falls back to the default in
    // silence, which is worse than an error.
    let num = |name: &str, default: f64| -> Result<f64> {
        let raw = pr.get_string(name, &default.to_string());
        raw.trim()
            .parse::<f64>()
            .map_err(|_| anyhow::anyhow!("property '{name}' is not a number: '{raw}'"))
    };
    let d = Settings::default();
    let cluster = num("cluster", d.cluster as f64)?.max(1.0) as usize;
    let n_quantiles = num("n_quantiles", d.n_quantiles as f64)?.max(3.0) as usize;
    let s = Settings {
        cluster,
        number_of_cells: num("number_of_cells", d.number_of_cells as f64)?.max(0.0) as usize,
        seed: num("seed", d.seed as f64)?.max(0.0) as u64,
        n_quantiles,
        min_cells: num("min_cells", d.min_cells as f64)?.max(2.0) as usize,
        batch_factor: pr.get_string("batch_factor", "").trim().to_string(),
        type_factor: pr.get_string("type_factor", "").trim().to_string(),
        reference_value: pr.get_string("reference_value", &d.reference_value),
        cluster_factor: pr.get_string("cluster_factor", "").trim().to_string(),
        collect_max_cells: num("collect_max_cells", d.collect_max_cells as f64)?.max(0.0) as usize,
    };
    if s.cluster > 1 && s.cluster_factor.is_empty() {
        bail!(
            "cluster = {} asks this operator to cluster the data with FlowSOM, which it does not \
             do: clustering is its own step. Add a FlowSOM step upstream — \
             flowsom_rust_operator, or the R flowsom_operator — project its metacluster column, \
             and name that factor in 'cluster_factor'. Or leave cluster at 1, which is CytoNorm \
             without clustering.",
            s.cluster
        );
    }
    if s.min_cells <= s.n_quantiles {
        bail!(
            "min_cells ({}) must exceed n_quantiles ({}): fewer reference cells than quantiles \
             cannot describe a distribution",
            s.min_cells,
            s.n_quantiles
        );
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_run_7_to_9_configuration() {
        let d = Settings::default();
        assert_eq!(d.n_quantiles, 99);
        assert_eq!(d.number_of_cells, 6000);
        assert_eq!(d.reference_value, "Train");
        assert_eq!(d.cluster, 1);
    }
}
