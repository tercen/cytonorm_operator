//! The result: one table, `.ri` / `.ci` / `<namespace>.asinh`, one row per cell.
//!
//! Shape is what `tercen-rs`'s `save_table` builds for a per-cell result — `{kind:
//! OperatorResult, tables: [table], joinOperators: []}` — but written as a **stream**, because
//! the result has exactly as many rows as the projected crosstab has cells and `save_table`
//! encodes the whole thing in memory first (create-rust-operator §4).
//!
//! TSON is column-major, so a column has to be written from start to end before the next one
//! begins. Two ways to do that:
//!
//! * **collect** (default under [`COLLECT_MAX_CELLS`]): one pass over the crosstab, keeping
//!   `.ri`, `.ci` and the transformed values (16 B/cell), then write.
//! * **stream**: three passes over the crosstab, one per column, keeping only the current chunk.
//!   Three times the server round trips, constant memory. Used above the threshold.
use std::io::Write;

use anyhow::{Result, anyhow};

use crate::tson::TsonWriter;

/// 20 M cells ≈ 320 MB of buffers, which fits comfortably under a 1 GB booking.
pub const COLLECT_MAX_CELLS: usize = 20_000_000;

pub struct ColSpec<'a> {
    pub name: &'a str,
    pub ty: &'a str, // "double" | "int32"
}

/// The three columns of the result.
///
/// The value comes **first** so the streaming path can write it straight through while it reads
/// the crosstab, and spill only the two index columns. Columns are addressed by name, so the
/// order is ours to choose; choosing it this way turns three passes over the input into one.
pub fn result_columns(namespace: &str) -> [ColSpec<'_>; 3] {
    [
        ColSpec {
            name: namespace,
            ty: "double",
        },
        ColSpec {
            name: ".ri",
            ty: "int32",
        },
        ColSpec {
            name: ".ci",
            ty: "int32",
        },
    ]
}

/// The result column's name: `<namespace>.cytonorm` (R `ctx$addNamespace()`).
pub fn value_column(namespace: &str) -> String {
    format!("{namespace}.cytonorm")
}

/// Name of the second output relation: the cofactors the run used.
pub const COFACTORS: &str = "Cofactors";

/// One row of the cofactor table — what was used, and how much to trust it.
#[derive(Debug, Clone)]
pub struct CofactorRow {
    pub channel: String,
    /// The cofactor applied to this channel.
    pub cofactor: f64,
    /// Bartlett's statistic at that cofactor. Large means flowVS found little to stabilise.
    pub objective: f64,
    /// `resolved`, `fragile`, `floored` or `unstable` — see `flowvs::estimate::Status`.
    pub status: String,
    /// What flowVS itself returned, before any floor.
    pub flowvs_cofactor: f64,
    /// `2.5 × σ_neg` when the channel has a negative population, else NaN.
    pub sigma_neg_cofactor: f64,
    /// The best cofactor from another interval of the search, and its objective, else NaN. A
    /// close second at a distant cofactor means the search chose between two minima.
    pub runner_up_cofactor: f64,
    pub runner_up_objective: f64,
    /// Cells per sample the estimate used, and the seed that chose them.
    pub cells_used: i32,
    pub seed: i32,
}

pub fn write_header<W: Write>(
    w: &mut TsonWriter<W>,
    table_name: &str,
    n_rows: usize,
    cols: &[ColSpec],
    n_tables: usize,
) -> Result<()> {
    w.map(3)?;
    w.key("kind")?;
    w.str("OperatorResult")?;
    w.key("tables")?;
    w.list(n_tables)?;

    w.map(4)?;
    w.key("kind")?;
    w.str("Table")?;
    w.key("nRows")?;
    w.i32(i32::try_from(n_rows).map_err(|_| {
        anyhow!(
            "the result would have {n_rows} rows, more than a Tercen table can hold (i32::MAX). \
             Project fewer cells, or split the step."
        )
    })?)?;
    w.key("properties")?;
    w.map(4)?;
    w.key("kind")?;
    w.str("TableProperties")?;
    w.key("name")?;
    w.str(table_name)?;
    w.key("sortOrder")?;
    w.list(0)?;
    w.key("ascending")?;
    w.bool(false)?;
    w.key("columns")?;
    w.list(cols.len())?;
    Ok(())
}

pub fn write_column_header<W: Write>(
    w: &mut TsonWriter<W>,
    c: &ColSpec,
    n_rows: usize,
) -> Result<()> {
    w.map(6)?;
    w.key("kind")?;
    w.str("Column")?;
    w.key("name")?;
    w.str(c.name)?;
    w.key("type")?;
    w.str(c.ty)?;
    w.key("nRows")?;
    w.i32(n_rows as i32)?;
    w.key("size")?;
    w.i32(n_rows as i32)?;
    w.key("values")?;
    Ok(())
}

/// The cofactor table, written after the per-cell table.
///
/// It exists so an estimate can be **reviewed and then frozen**: read it, look at the
/// histograms, and feed it back as the cofactor row factor with `method = manual`. Without that
/// round trip an automatic estimate silently changes whenever the data flowing through the step
/// changes, and two timepoints stop being comparable.
pub fn write_cofactor_table<W: Write>(w: &mut TsonWriter<W>, rows: &[CofactorRow]) -> Result<()> {
    let cols = [
        ColSpec {
            name: "channel",
            ty: "string",
        },
        ColSpec {
            name: "cofactor",
            ty: "double",
        },
        ColSpec {
            name: "bartlett",
            ty: "double",
        },
        ColSpec {
            name: "status",
            ty: "string",
        },
        ColSpec {
            name: "flowvs_cofactor",
            ty: "double",
        },
        ColSpec {
            name: "sigma_neg_cofactor",
            ty: "double",
        },
        ColSpec {
            name: "runner_up_cofactor",
            ty: "double",
        },
        ColSpec {
            name: "runner_up_bartlett",
            ty: "double",
        },
        ColSpec {
            name: "cells_used",
            ty: "int32",
        },
        ColSpec {
            name: "seed",
            ty: "int32",
        },
    ];
    let n = rows.len();
    w.map(4)?;
    w.key("kind")?;
    w.str("Table")?;
    w.key("nRows")?;
    w.i32(i32::try_from(n).unwrap_or(i32::MAX))?;
    w.key("properties")?;
    w.map(4)?;
    w.key("kind")?;
    w.str("TableProperties")?;
    w.key("name")?;
    w.str(COFACTORS)?;
    w.key("sortOrder")?;
    w.list(0)?;
    w.key("ascending")?;
    w.bool(false)?;
    w.key("columns")?;
    w.list(cols.len())?;

    let f64_col =
        |w: &mut TsonWriter<W>, i: usize, f: &dyn Fn(&CofactorRow) -> f64| -> Result<()> {
            write_column_header(w, &cols[i], n)?;
            w.f64_list(&rows.iter().map(f).collect::<Vec<_>>())?;
            Ok(())
        };
    write_column_header(w, &cols[0], n)?;
    w.str_list(&rows.iter().map(|r| r.channel.as_str()).collect::<Vec<_>>())?;
    f64_col(w, 1, &|r| r.cofactor)?;
    f64_col(w, 2, &|r| r.objective)?;
    write_column_header(w, &cols[3], n)?;
    w.str_list(&rows.iter().map(|r| r.status.as_str()).collect::<Vec<_>>())?;
    f64_col(w, 4, &|r| r.flowvs_cofactor)?;
    f64_col(w, 5, &|r| r.sigma_neg_cofactor)?;
    f64_col(w, 6, &|r| r.runner_up_cofactor)?;
    f64_col(w, 7, &|r| r.runner_up_objective)?;
    write_column_header(w, &cols[8], n)?;
    w.i32_list(&rows.iter().map(|r| r.cells_used).collect::<Vec<_>>())?;
    write_column_header(w, &cols[9], n)?;
    w.i32_list(&rows.iter().map(|r| r.seed).collect::<Vec<_>>())?;
    Ok(())
}

/// Append `i32`s in the little-endian layout that TSON and the spill files share.
pub fn write_i32_le<W: Write>(out: &mut W, v: &[i32]) -> anyhow::Result<()> {
    #[cfg(target_endian = "little")]
    {
        // SAFETY: i32 has no padding, so its bytes are exactly the encoding wanted here.
        let bytes = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 4) };
        out.write_all(bytes)?;
    }
    #[cfg(not(target_endian = "little"))]
    for x in v {
        out.write_all(&x.to_le_bytes())?;
    }
    Ok(())
}

/// Pour a spill file into the result, refusing to if it does not hold what the header promised.
pub fn pour<W: Write>(w: &mut TsonWriter<W>, path: &std::path::Path, expect: usize) -> Result<()> {
    use std::io::Read as _;
    let len = std::fs::metadata(path)?.len() as usize;
    if len != expect {
        return Err(anyhow!(
            "{} holds {len} bytes where the result declares {expect}",
            path.display()
        ));
    }
    let mut f = std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(path)?);
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        w.raw(&buf[..n])?;
    }
    Ok(())
}

fn write_simple_relation<W: Write>(w: &mut TsonWriter<W>, id: &str) -> Result<()> {
    w.map(3)?;
    w.key("kind")?;
    w.str("SimpleRelation")?;
    w.key("id")?;
    w.str(id)?;
    w.key("index")?;
    w.i32(0)?;
    Ok(())
}

fn write_column_pair<W: Write>(w: &mut TsonWriter<W>, l: &[&str], r: &[&str]) -> Result<()> {
    w.map(3)?;
    w.key("kind")?;
    w.str("ColumnPair")?;
    w.key("lColumns")?;
    w.list(l.len())?;
    for s in l {
        w.str(s)?;
    }
    w.key("rColumns")?;
    w.list(r.len())?;
    for s in r {
        w.str(s)?;
    }
    Ok(())
}

/// Close the result. With a cofactor table there is one join, declaring it as a standalone
/// relation beside the per-cell one (the shape read_fcs uses for its summary table).
pub fn write_footer<W: Write>(w: &mut TsonWriter<W>, with_cofactors: bool) -> Result<()> {
    w.key("joinOperators")?;
    w.list(usize::from(with_cofactors))?;
    if with_cofactors {
        w.map(4)?;
        w.key("kind")?;
        w.str("JoinOperator")?;
        w.key("joinType")?;
        w.str("")?;
        w.key("leftPair")?;
        write_column_pair(w, &[], &[])?;
        w.key("rightRelation")?;
        write_simple_relation(w, COFACTORS)?;
    }
    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_value_column_carries_the_namespace() {
        assert_eq!(value_column("ds0"), "ds0.cytonorm");
    }

    #[test]
    fn a_result_bigger_than_an_i32_is_an_error_not_a_panic() {
        let mut w = TsonWriter::new(Vec::new()).unwrap();
        let cols = result_columns("ds0.asinh");
        let e = write_header(&mut w, "t", i32::MAX as usize + 1, &cols, 1).unwrap_err();
        assert!(e.to_string().contains("more than a Tercen table can hold"));
    }
}
