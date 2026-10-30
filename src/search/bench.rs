//! The bench: every search kind on the fixed positions, side by side.

use super::agreement::agree;
use super::positions::bench_positions;
use super::{engage, Search, SearchKind};
use crate::solver::Outcome;
use std::time::Duration;

/// One kind's totals over all positions.
pub struct Row {
    /// The kind's name.
    pub name: &'static str,
    /// The number of positions searched.
    pub positions: usize,
    /// The evaluations summed over the positions.
    pub evaluations: usize,
    /// The cuts summed over the positions.
    pub cuts: usize,
    /// The search time summed over the positions.
    pub time: Duration,
    /// The positions where the result agrees with `basic`.
    pub matches: usize,
    /// The labels of the positions that do not.
    pub mismatches: Vec<String>,
}

/// Searches every bench position at `depth` with each kind and totals what
/// each did.
pub fn run(depth: usize) -> Vec<Row> {
    let positions = bench_positions(depth);
    let search = |kind| {
        let search = Search { kind, depth };
        positions
            .iter()
            .map(|p| engage(&p.conflict, search))
            .collect::<Vec<Outcome>>()
    };
    let basic = search(SearchKind::Basic);
    SearchKind::ALL
        .into_iter()
        .map(|kind| match kind {
            SearchKind::Basic => total(kind.name(), &positions, &basic, &basic),
            _ => total(kind.name(), &positions, &search(kind), &basic),
        })
        .collect()
}

/// Totals searched `outcomes`, checking each against the `basic` result.
fn total(
    name: &'static str,
    positions: &[super::positions::BenchPosition],
    outcomes: &[Outcome],
    basic: &[Outcome],
) -> Row {
    let mut row = Row {
        name,
        positions: positions.len(),
        evaluations: 0,
        cuts: 0,
        time: Duration::ZERO,
        matches: 0,
        mismatches: Vec::new(),
    };
    for ((position, outcome), reference) in positions.iter().zip(outcomes).zip(basic) {
        row.evaluations += outcome.evaluations;
        row.cuts += outcome.cuts;
        row.time += outcome.search_duration;
        if agree(outcome, reference) {
            row.matches += 1;
        } else {
            row.mismatches.push(position.label.clone());
        }
    }
    row
}

/// Lays the rows out as a table, with the mismatches listed below it.
pub fn render(rows: &[Row], depth: usize) -> String {
    let positions = rows.first().map_or(0, |row| row.positions);
    let basic = rows
        .iter()
        .find(|row| row.name == SearchKind::Basic.name())
        .map(|row| row.evaluations);

    let cells: Vec<[String; 6]> = rows
        .iter()
        .map(|row| {
            let ratio = match basic {
                Some(basic) if basic > 0 => {
                    format!("{:.2}×", row.evaluations as f64 / basic as f64)
                }
                _ => "n/a".to_string(),
            };
            [
                row.name.to_string(),
                row.evaluations.to_string(),
                ratio,
                row.cuts.to_string(),
                format!("{:.1}ms", row.time.as_secs_f64() * 1000.0),
                format!("{}/{}", row.matches, row.positions),
            ]
        })
        .collect();
    let header = ["kind", "evaluations", "×basic", "cuts", "time", "match"].map(String::from);
    let width = |column: usize| {
        cells
            .iter()
            .map(|cells| cells[column].chars().count())
            .chain([header[column].chars().count()])
            .max()
            .unwrap_or(0)
    };
    let widths: Vec<usize> = (0..6).map(width).collect();
    let line = |cells: &[String; 6]| {
        let mut line = format!("{:<w$}", cells[0], w = widths[0]);
        for column in 1..6 {
            line.push_str(&format!("  {:>w$}", cells[column], w = widths[column]));
        }
        line
    };

    let mut lines = vec![
        format!("search bench · depth {depth} · {positions} positions"),
        line(&header),
    ];
    lines.extend(cells.iter().map(line));
    for row in rows {
        for label in &row.mismatches {
            lines.push(format!("  {}: {label}", row.name));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_layout() {
        let rows = [
            Row {
                name: "basic",
                positions: 3,
                evaluations: 1000,
                cuts: 200,
                time: Duration::from_micros(12_340),
                matches: 3,
                mismatches: Vec::new(),
            },
            Row {
                name: "tt",
                positions: 3,
                evaluations: 420,
                cuts: 90,
                time: Duration::from_millis(5),
                matches: 2,
                mismatches: vec!["duel seed 1 @ 3".to_string()],
            },
        ];
        let expected = "\
search bench · depth 6 · 3 positions
kind   evaluations  ×basic  cuts    time  match
basic         1000   1.00×   200  12.3ms    3/3
tt             420   0.42×    90   5.0ms    2/3
  tt: duel seed 1 @ 3";
        assert_eq!(render(&rows, 6), expected);
    }
}
