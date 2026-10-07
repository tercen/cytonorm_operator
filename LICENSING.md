# Licensing

`cytonorm_operator` is **GPL-2.0-or-later**.

It was AGPL-3.0 until 2026-09-21. The change is not cosmetic: it is what lets the operator link
[`flowsom-rs`](https://github.com/tercen/flowsom-rs) and so cluster, and it was made deliberately
rather than discovered later.

## Why

The clustering CytoNorm needs is FlowSOM's, and FlowSOM's metaclustering is
ConsensusClusterPlus, which is licensed **"GPL version 2"** with no "or later". A port of it
inherits that. GPL-2-only and AGPL-3 are incompatible, so an AGPL-3 operator cannot link it at
all — there is no combination of the two that may be distributed.

GPL-2-or-later can. The distributed combination of this operator and `flowsom-rs` is then
GPL-2-only in effect, which is exactly what ConsensusClusterPlus asks for.

Nothing else here objected: the algorithm was ported from **cytonormpy, which is MIT**, and the
Tercen SDK crates are MIT.

## What each part is under

| what | licence | why it matters |
|---|---|---|
| this operator | GPL-2.0-or-later | can link either a GPL-2-only or a GPL-3 crate |
| `flowsom-rs` | GPL-2.0-only | pinned by ConsensusClusterPlus |
| FlowSOM, R's `hclust` | GPL (>= 2) | would allow GPL-3; not the binding constraint |
| cytonormpy (the reference this was ported from) | MIT | no obligation |
| CytoNorm (R) | GPL (>= 2) | not ported here |
| tercen-rs | Apache-2.0 (since 2026-10-08) | GPL-3 compatible; with or-later the binary is distributable as GPL-3 |
| rustson | none yet (tercen/rustson#1 proposes Apache-2.0) | pending |

## The sibling operators

`asinh_rust_operator` and `flowvs-rs` moved from AGPL-3.0 to GPL-2.0-or-later on 2026-10-08, so
asinh could link FlowSOM if it ever needs to. `umap_rust_operator` moved from AGPL-3.0 to Apache-2.0. `read_fcs_rust_operator` is
Apache-2.0 and has no copyleft dependency at all.
