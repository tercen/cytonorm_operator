"""Compare a CytoNorm result exported from Tercen with cytonormpy's own output.

    python dev/compare_to_python.py <result.csv> <rowtable.csv> <coltable.csv> <normalized.csv>

The point of comparing the *platform's* result rather than the in-process one: it covers the
projection, the column factors, the TSON the operator wrote and the server's reading of it.
"""
import csv, sys

res, rowt, colt, py = (list(csv.DictReader(open(p))) for p in sys.argv[1:5])
channels = [r["channel"] for r in rowt]
cells = [int(float(r["cell_id"])) for r in colt]
val_col = next(c for c in res[0] if c.endswith(".cytonorm"))

worst, worst_at, n = 0.0, "", 0
for r in res:
    ch = channels[int(float(r[".ri"]))]
    cell = cells[int(float(r[".ci"]))]
    got = float(r[val_col])
    want = float(py[cell][ch])
    rel = abs(got - want) / max(abs(want), 1e-6)
    if rel > worst:
        worst, worst_at = rel, f"{ch} cell {cell}"
    n += 1
print(f"{n:,} values compared against cytonormpy")
print(f"worst relative difference {worst:.3e}" + (f" at {worst_at}" if worst_at else ""))
ok = n == len(py) * len(channels) and worst <= 1e-9
print("PARITY OK" if ok else "PARITY FAILED")
sys.exit(0 if ok else 1)
