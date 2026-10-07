#!/usr/bin/env python
"""Ground truth from cytonormpy **with clusters**, which `gen_fixtures.py` does not cover.

    /tmp/claude-1000/tercenv/bin/python fixtures/gen_cluster_fixtures.py

The operator takes cluster labels as an input factor rather than clustering itself, so the
clusterer only has to be something reproducible — KMeans, not FlowSOM. What is being checked is
that a spline is fitted per (batch, cluster, channel) and applied to the right cells, which is
the part `gen_fixtures.py` cannot reach with its single cluster.

Synthetic data only.
"""
import json, os
import numpy as np, pandas as pd, anndata as ad, cytonormpy as cnp
from scipy.interpolate import PchipInterpolator
import cytonormpy._normalization._spline_calc as _SC

OUT = os.path.dirname(os.path.abspath(__file__))
N_QUANTILES = 99
N_CLUSTERS = 3

# The spline configuration runs 7-9 use: PCHIP tangents, slope-1 extrapolation.
_SC.Spline._select_interpolants = lambda self, x, y: PchipInterpolator(x, y)(x, nu=1)

def _slope1_extrapolation(self):
    for side in ("left", "right"):
        x0 = self.fit_func.x[0] if side == "left" else self.fit_func.x[-1]
        y0 = self.fit_func(x0)
        xn = np.nextafter(x0, x0 - 1) if side == "left" else np.nextafter(x0, x0 + 1)
        self.fit_func.extend(np.array([0, 0, 1.0, y0 + (xn - x0)])[..., None], np.r_[xn])

_SC.Spline._extrapolate_linear = _slope1_extrapolation

# With a clusterer attached, cytonormpy hands numba a read-only view and its signature does not
# accept one: "No matching definition for argument type(s) readonly array(float64, 2d, A)".
# Copying at the boundary changes no arithmetic.
import cytonormpy._normalization._utils as _U
_orig_quantiles = _U.numba_quantiles

def _writable_quantiles(a, q):
    return _orig_quantiles(np.ascontiguousarray(a), np.ascontiguousarray(q))

_U.numba_quantiles = _writable_quantiles
import cytonormpy._normalization._quantile_calc as _QC
if hasattr(_QC, "numba_quantiles"):
    _QC.numba_quantiles = _writable_quantiles

rng = np.random.default_rng(20260921)
channels = ["CD3", "CD4", "CD8"]
batches = ["b1", "b2", "b3"]
shift = {"b1": 1.0, "b2": 1.35, "b3": 0.8}
rows, obs = [], []
for b in batches:
    for kind, n_files in (("Train", 2), ("validate", 2)):
        for f in range(n_files):
            name = f"{b}_{kind}{f}.fcs"
            n = 3000
            # Three populations, so the clusterer has something real to find.
            which = rng.integers(0, 3, size=n)
            centres = np.array([[50, 80, 120], [900, 1800, 3000], [400, 200, 1500]], dtype=float)
            spread = np.array([[30, 40, 60], [250, 600, 900], [150, 80, 500]], dtype=float)
            x = rng.normal(centres[which], spread[which]) * shift[b]
            rows.append(np.arcsinh(x / 150.0))
            obs.append(pd.DataFrame({
                "file_name": [name] * n,
                "reference": ["ref" if kind == "Train" else "other"] * n,
                "batch": [b] * n,
                "type": [kind] * n,
            }))

X = np.ascontiguousarray(np.vstack(rows), dtype=np.float64)
obs = pd.concat(obs, ignore_index=True)
obs.index = pd.Index([f"{s}|{i}" for i, s in enumerate(obs.file_name)])
A = ad.AnnData(X=X, obs=obs, var=pd.DataFrame(index=channels))
A.layers["compensated"] = A.X.copy()

cn = cnp.CytoNorm()
cn.run_anndata_setup(A, layer="compensated", reference_column="reference", reference_value="ref",
                     batch_column="batch", sample_identifier_column="file_name",
                     channels=channels, key_added="norm")
cn.add_clusterer(cnp.KMeans(n_clusters=N_CLUSTERS, random_state=0, n_init=10))
cn.run_clustering(n_cells=None, test_cluster_cv=False)
qa = [i / (N_QUANTILES + 1) for i in range(1, N_QUANTILES + 1)]
cn.calculate_quantiles(n_quantiles=N_QUANTILES, quantile_array=qa)
cn.calculate_splines(goal="batch_mean")
cn.normalize_data()

# The cluster of every cell, which is what the operator receives as a factor. cytonormpy keeps
# the reference cells' clusters on an index level and clusters each validation file as it
# normalises it, so ask the clusterer directly, the same way it does.
labels = cn._clustering.calculate_clusters(X=np.ascontiguousarray(A.X))

inp = pd.DataFrame(A.X, columns=channels)
inp["file_name"] = A.obs.file_name.values
inp["batch"] = A.obs.batch.values
inp["type"] = A.obs.type.values
inp["cluster"] = [str(v) for v in labels]
inp.to_csv(f"{OUT}/cluster_input.csv", index=False)

out = pd.DataFrame(np.asarray(A.layers["norm"]), columns=channels)
out["file_name"] = A.obs.file_name.values
out.to_csv(f"{OUT}/cluster_normalized.csv", index=False)

json.dump({"n_quantiles": N_QUANTILES, "quantile_probs": qa, "goal": "batch_mean",
           "clusterer": f"KMeans(n_clusters={N_CLUSTERS}, random_state=0)",
           "batches": [str(b) for b in cn.batches],
           "clusters": [str(c) for c in cn.clusters],
           "channels": list(cn.channels)},
          open(f"{OUT}/cluster_config.json", "w"), indent=1)
print("batches", list(cn.batches), "clusters", list(cn.clusters))
print("rows", len(inp), "cluster sizes", pd.Series(labels).value_counts().to_dict())
