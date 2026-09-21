# Licensing

`cytonorm_rust_operator` is **GPL-2.0-or-later**.

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
| tercen-rs, rustson | MIT | no obligation |

## The sibling operators

`asinh_rust_operator` and `flowvs-rs` are AGPL-3.0 and stay that way: they have no GPL-2-only
dependency, so nothing forces a change. Note for later — **if asinh ever wants FlowSOM, it hits
this same wall** and would have to move to GPL-2-or-later first. `read_fcs_rust_operator` is
Apache-2.0 and has no copyleft dependency at all.
