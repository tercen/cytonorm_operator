#!/usr/bin/env python
"""Ground truth from cytonormpy, configured the way runs 7-9 configure it.

    .venv/bin/python fixtures/gen_fixtures.py

Synthetic data only: three batches, two conditions, a handful of channels with a batch shift.
Dumps the inputs, the per-batch quantiles, the goal, and the normalised values, so the Rust port
can be checked stage by stage rather than only at the end.
"""
import json, os, sys
import numpy as np, pandas as pd, anndata as ad, cytonormpy as cnp
from scipy.interpolate import PchipInterpolator
import cytonormpy._normalization._spline_calc as _SC

OUT = os.path.dirname(os.path.abspath(__file__))
N_QUANTILES = 99          # runs 7-9; the notebook default is 101

# --- the spline patch runs 7-9 use: PCHIP tangents, slope-1 extrapolation -------------------
def _pchip_tangents(self, x, y):
    return PchipInterpolator(x, y)(x, nu=1)

def _slope1_extrapolation(self):
    for side in ("left", "right"):
        x0 = self.fit_func.x[0] if side == "left" else self.fit_func.x[-1]
        y0 = self.fit_func(x0)
        xn = np.nextafter(x0, x0 - 1) if side == "left" else np.nextafter(x0, x0 + 1)
        self.fit_func.extend(np.array([0, 0, 1.0, y0 + (xn - x0)])[..., None], np.r_[xn])

_SC.Spline._select_interpolants = _pchip_tangents
_SC.Spline._extrapolate_linear = _slope1_extrapolation

# --- synthetic data --------------------------------------------------------------------------
rng = np.random.default_rng(20260920)
channels = ["CD3", "CD4", "CD8"]
batches = ["b1", "b2", "b3"]
shift = {"b1": 1.0, "b2": 1.35, "b3": 0.8}      # a multiplicative batch effect
rows, obs = [], []
for b in batches:
    for kind, n_files in (("Train", 2), ("validate", 2)):
        for f in range(n_files):
            name = f"{b}_{kind}{f}.fcs"
            n = 4000
            neg = rng.normal(0, 30, size=(n, len(channels)))
            pos_mask = rng.random((n, len(channels))) < 0.3
            pos = rng.normal([900, 1800, 3000], [250, 600, 900], size=(n, len(channels)))
            x = np.where(pos_mask, pos, neg) * shift[b]
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
qa = [i / (N_QUANTILES + 1) for i in range(1, N_QUANTILES + 1)]
cn.calculate_quantiles(n_quantiles=N_QUANTILES, quantile_array=qa)
cn.calculate_splines(goal="batch_mean")
cn.normalize_data()

# --- dump ------------------------------------------------------------------------------------
inp = pd.DataFrame(A.X, columns=channels)
inp["file_name"] = A.obs.file_name.values
inp["batch"] = A.obs.batch.values
inp["type"] = A.obs.type.values
inp.to_csv(f"{OUT}/input.csv", index=False)

out = pd.DataFrame(np.asarray(A.layers["norm"]), columns=channels)
out["file_name"] = A.obs.file_name.values
out.to_csv(f"{OUT}/normalized.csv", index=False)

eq = cn._expr_quantiles
gd = cn._goal_distrib
recs = []
for b, batch in enumerate(cn.batches):
    for c, cluster in enumerate(cn.clusters):
        for ch, channel in enumerate(cn.channels):
            q = eq.get_quantiles(channel_idx=ch, quantile_idx=None, cluster_idx=c, batch_idx=b).ravel()
            g = gd.get_quantiles(channel_idx=ch, quantile_idx=None, cluster_idx=c, batch_idx=None).ravel()
            for i, (qi, gi) in enumerate(zip(q, g)):
                recs.append({"batch": batch, "cluster": cluster, "channel": channel,
                             "i": i, "p": qa[i], "q": float(qi), "goal": float(gi)})
pd.DataFrame(recs).to_csv(f"{OUT}/quantiles.csv", index=False)

json.dump({"n_quantiles": N_QUANTILES, "quantile_probs": qa, "goal": "batch_mean",
           "spline": "pchip tangents + slope-1 extrapolation",
           "batches": [str(b) for b in cn.batches],
           "clusters": [str(c) for c in cn.clusters],
           "channels": list(cn.channels)},
          open(f"{OUT}/config.json", "w"), indent=1)
print(f"batches {list(cn.batches)} clusters {list(cn.clusters)} channels {list(cn.channels)}")
print(f"rows {len(inp):,}  quantile rows {len(recs):,}")
