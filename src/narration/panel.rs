//! Party status panel: one row per member with a health bar.

use crate::conflict::Conflict;
use crate::party_member::Morale;

/// Number of cells in a health bar.
const BAR_WIDTH: usize = 20;

/// Renders a fixed-width health bar of `█` (filled) and `░` (empty) cells.
///
/// At least one cell is filled while `health > 0`.
pub(crate) fn health_bar(health: f32, max: f32) -> String {
    let filled = filled_cells(health, max, BAR_WIDTH);
    format!("{}{}", "█".repeat(filled), "░".repeat(BAR_WIDTH - filled))
}

/// The number of filled cells in a health bar `width` cells wide.
///
/// At least one cell is filled while `health > 0`.
pub(crate) fn filled_cells(health: f32, max: f32, width: usize) -> usize {
    let ratio = if max > 0.0 { health / max } else { 0.0 };
    let mut filled = ((ratio * width as f32).round() as i64).clamp(0, width as i64) as usize;
    if health > 0.0 {
        filled = filled.max(1);
    }
    filled
}

/// Describes a morale worth showing; `Tactical` is the default and stays silent.
pub(crate) fn morale_tag(morale: Morale) -> Option<String> {
    match morale {
        Morale::Tactical => None,
        Morale::Never => Some("never flees".to_string()),
        Morale::Courage(p) => Some(format!("flees at ≤{}%", (p * 100.0).round() as u32)),
        Morale::AfterRounds(n) => Some(format!("flees in round {}", n + 1)),
    }
}

/// Builds the status rows for all members: initiator party first, then the
/// opponent, each ordered by member ID.
///
/// The maximum health shown is the member's health in `start`.
pub(crate) fn panel_lines(start: &Conflict, now: &Conflict, names: &[Vec<String>]) -> Vec<String> {
    let width = names
        .iter()
        .flatten()
        .map(|n| n.chars().count())
        .max()
        .unwrap_or(0);

    let mut rows = Vec::new();
    for (start_party, now_party) in [
        (&start.initiator, &now.initiator),
        (&start.opponent, &now.opponent),
    ] {
        for member in &now_party.members {
            let max = start_party
                .members
                .iter()
                .find(|m| m.id == member.id)
                .map_or(member.health, |m| m.health);
            let name = &names[now_party.id][member.id];
            let status = if member.is_dead() {
                "✝ fallen".to_string()
            } else if member.fled {
                "↩ fled".to_string()
            } else {
                let mut status = format!(
                    "{} {}/{}",
                    health_bar(member.health, max),
                    member.health,
                    max
                );
                if let Some(tag) = morale_tag(member.morale) {
                    status.push_str(&format!("  {tag}"));
                }
                status
            };
            rows.push(format!("  {name:<width$}  {status}"));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::Conflict;
    use crate::party::Party;
    use crate::party_member::{Morale, PartyMember};
    use crate::weapon::{Fists, Weapon};

    fn member(id: usize, health: f32) -> PartyMember {
        PartyMember {
            id,
            health,
            damage_taken: 0.0,
            weapon: Weapon::Fists(Fists { damage: 1.0 }),
            can_act: true,
            speed: 1,
            morale: Morale::Tactical,
            fled: false,
        }
    }

    fn party(id: usize, members: Vec<PartyMember>) -> Party {
        Party {
            id,
            members,
            can_retreat: true,
        }
    }

    fn names(groups: &[&[&str]]) -> Vec<Vec<String>> {
        groups
            .iter()
            .map(|g| g.iter().map(|n| n.to_string()).collect())
            .collect()
    }

    #[test]
    fn health_bar_full_half_empty() {
        assert_eq!(health_bar(20.0, 20.0), "█".repeat(20));
        assert_eq!(
            health_bar(10.0, 20.0),
            format!("{}{}", "█".repeat(10), "░".repeat(10))
        );
        assert_eq!(health_bar(0.0, 20.0), "░".repeat(20));
    }

    #[test]
    fn health_bar_is_always_twenty_cells() {
        for (h, m) in [(14.0, 15.0), (0.4, 20.0), (25.0, 20.0)] {
            assert_eq!(health_bar(h, m).chars().count(), 20);
        }
        assert!(health_bar(0.4, 20.0).starts_with('█'));
    }

    #[test]
    fn filled_cells_scale_to_the_width() {
        assert_eq!(filled_cells(20.0, 20.0, 10), 10);
        assert_eq!(filled_cells(12.0, 20.0, 10), 6);
        assert_eq!(filled_cells(0.4, 20.0, 10), 1);
        assert_eq!(filled_cells(0.0, 20.0, 10), 0);
    }

    #[test]
    fn panel_marks_fallen_and_fled() {
        let names = names(&[&["Brull"], &["Ziuon", "Molphige"]]);
        let start = Conflict::new(
            party(0, vec![member(0, 20.0)]),
            party(1, vec![member(0, 15.0), member(1, 10.0)]),
        );
        let mut now = Conflict::new(
            party(0, vec![member(0, 15.0)]),
            party(1, vec![member(0, 15.0), member(1, 0.0)]),
        );
        assert_eq!(
            panel_lines(&start, &now, &names),
            vec![
                format!("  Brull     {} 15/20", health_bar(15.0, 20.0)),
                format!("  Ziuon     {} 15/15", health_bar(15.0, 15.0)),
                "  Molphige  ✝ fallen".to_string(),
            ]
        );

        now.opponent.retreat();
        let rows = panel_lines(&start, &now, &names);
        assert_eq!(rows[1], "  Ziuon     ↩ fled");
        assert_eq!(rows[2], "  Molphige  ✝ fallen");
    }

    #[test]
    fn morale_tags() {
        assert_eq!(morale_tag(Morale::Tactical), None);
        assert_eq!(morale_tag(Morale::Never).as_deref(), Some("never flees"));
        assert_eq!(
            morale_tag(Morale::Courage(0.4)).as_deref(),
            Some("flees at ≤40%")
        );
        assert_eq!(
            morale_tag(Morale::AfterRounds(2)).as_deref(),
            Some("flees in round 3")
        );
    }

    #[test]
    fn panel_marks_fled_members_individually() {
        let names = names(&[&["Brull"], &["Ziuon", "Molphige"]]);
        let start = Conflict::new(
            party(0, vec![member(0, 20.0)]),
            party(1, vec![member(0, 15.0), member(1, 10.0)]),
        );
        let mut now = start.clone();
        now.opponent.members[1].fled = true;
        now.opponent.members[0].morale = Morale::Courage(0.3);
        let rows = panel_lines(&start, &now, &names);
        assert_eq!(
            rows[1],
            format!(
                "  Ziuon     {} 15/15  flees at ≤30%",
                health_bar(15.0, 15.0)
            )
        );
        assert_eq!(rows[2], "  Molphige  ↩ fled");
        assert!(rows[0].contains('█'));
    }

    #[test]
    fn panel_aligns_non_ascii_names() {
        let names = names(&[&["Al"], &["Örshashee"]]);
        let start = Conflict::new(
            party(0, vec![member(0, 5.0)]),
            party(1, vec![member(0, 5.0)]),
        );
        let rows = panel_lines(&start, &start, &names);
        let col = |r: &String| {
            r.chars()
                .position(|c| c == '█' || c == '░' || c == '✝' || c == '↩')
                .unwrap()
        };
        assert_eq!(col(&rows[0]), col(&rows[1]));
    }
}
