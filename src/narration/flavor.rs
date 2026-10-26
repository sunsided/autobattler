//! Plain-text flavor for the combat log.

use crate::action::SimpleAttackAction;
use crate::solver::OutcomeType;
use crate::weapon::Weapon;

/// Picks a deterministic entry from a table based on the round and the actor's slot.
fn pick(table: &[&'static str], round: usize, slot: usize) -> &'static str {
    table[(round * 7 + slot * 3) % table.len()]
}

/// Chooses the verb describing an attack, varying with round and slot.
pub(crate) fn attack_verb(action: &SimpleAttackAction, round: usize, slot: usize) -> &'static str {
    match action.weapon {
        Some(Weapon::Fists(_)) => pick(&["punches", "jabs", "lands a haymaker on"], round, slot),
        Some(Weapon::Stick(_)) => pick(&["whacks", "jabs at", "smacks"], round, slot),
        None => pick(&["glares at", "stares down"], round, slot),
    }
}

/// Describes the tool used for the attack, with a leading space, or nothing.
pub(crate) fn instrument(action: &SimpleAttackAction) -> &'static str {
    match action.weapon {
        Some(Weapon::Stick(_)) => " with a stick",
        _ => "",
    }
}

/// Chooses the phrase announcing a defeated combatant, varying with round and slot.
pub(crate) fn kill_phrase(round: usize, slot: usize) -> &'static str {
    pick(&["collapses!", "is down!", "falls!"], round, slot)
}

/// The base form of the attack verb, used for tactician labels.
pub(crate) fn base_verb(action: &SimpleAttackAction) -> &'static str {
    match action.weapon {
        Some(Weapon::Fists(_)) => "punch",
        Some(Weapon::Stick(_)) => "whack",
        None => "glare at",
    }
}

/// Joins names naturally: "A", "A and B", "A, B and C".
pub(crate) fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_string(),
        [init @ .., last] => format!("{} and {}", init.join(", "), last),
    }
}

/// The opening line of a fight.
pub(crate) fn intro(heroes: &[&str], villains: &[&str]) -> String {
    let verb = if heroes.len() > 1 { "stand" } else { "stands" };
    format!(
        "⚔  {} {} against {}.",
        join_names(heroes),
        verb,
        join_names(villains)
    )
}

/// The closing line of a fight.
pub(crate) fn outro(
    outcome: &OutcomeType,
    heroes: &[&str],
    villains: &[&str],
    rounds: usize,
) -> String {
    let hold = if heroes.len() > 1 { "hold" } else { "holds" };
    let heroes = join_names(heroes);
    let villains = join_names(villains);
    let n = format!("{} round{}", rounds, if rounds == 1 { "" } else { "s" });
    match outcome {
        OutcomeType::Win(_) => format!("🏁 Victory for {heroes} after {n}."),
        OutcomeType::Lose(_) => format!("🏁 {heroes} fell after {n}."),
        OutcomeType::Remain(_) => {
            format!("🏁 {villains} fled; {heroes} {hold} the field after {n}.")
        }
        OutcomeType::Retreat(_) => format!("🏁 {heroes} escaped after {n}."),
        OutcomeType::Unknown(_) => {
            format!("🏁 Undecided after {n} — the tactician could not see further.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::{Fists, Stick};
    use std::collections::HashSet;

    fn fists(d: f32) -> SimpleAttackAction {
        SimpleAttackAction {
            weapon: Some(Weapon::Fists(Fists { damage: d })),
            damage: d,
        }
    }

    fn stick(d: f32) -> SimpleAttackAction {
        SimpleAttackAction {
            weapon: Some(Weapon::Stick(Stick { damage: d })),
            damage: d,
        }
    }

    fn stare() -> SimpleAttackAction {
        SimpleAttackAction {
            weapon: None,
            damage: 0.0,
        }
    }

    #[test]
    fn verbs_are_deterministic() {
        assert_eq!(
            attack_verb(&fists(10.0), 1, 0),
            attack_verb(&fists(10.0), 1, 0)
        );
    }

    #[test]
    fn verbs_vary_across_turns() {
        let all: HashSet<_> = (1..4)
            .flat_map(|r| (0..3).map(move |s| attack_verb(&fists(10.0), r, s)))
            .collect();
        assert!(all.len() > 1);
    }

    #[test]
    fn verb_index_follows_formula() {
        // (1 * 7 + 1 * 3) % 3 == 1
        assert_eq!(attack_verb(&fists(1.0), 1, 1), "jabs");
        assert_eq!(attack_verb(&stick(1.0), 0, 0), "whacks");
        // (1 * 7) % 2 == 1
        assert_eq!(attack_verb(&stare(), 1, 0), "stares down");
        assert_eq!(kill_phrase(1, 0), "is down!");
    }

    #[test]
    fn stick_has_instrument() {
        assert_eq!(instrument(&stick(5.0)), " with a stick");
        assert_eq!(instrument(&fists(1.0)), "");
        assert_eq!(instrument(&stare()), "");
    }

    #[test]
    fn base_verbs() {
        assert_eq!(base_verb(&fists(1.0)), "punch");
        assert_eq!(base_verb(&stick(1.0)), "whack");
        assert_eq!(base_verb(&stare()), "glare at");
    }

    #[test]
    fn names_join_naturally() {
        assert_eq!(join_names(&["A"]), "A");
        assert_eq!(join_names(&["A", "B"]), "A and B");
        assert_eq!(join_names(&["A", "B", "C"]), "A, B and C");
    }

    #[test]
    fn intro_agrees_in_number() {
        assert_eq!(
            intro(&["Brull"], &["Ziuon", "Molphige"]),
            "⚔  Brull stands against Ziuon and Molphige."
        );
        assert_eq!(intro(&["A", "B"], &["C"]), "⚔  A and B stand against C.");
    }

    #[test]
    fn outro_pluralizes_rounds() {
        assert_eq!(
            outro(&OutcomeType::Win(10.0), &["Brull"], &["Z"], 3),
            "🏁 Victory for Brull after 3 rounds."
        );
        assert_eq!(
            outro(&OutcomeType::Lose(-20.0), &["Brull"], &["Z"], 1),
            "🏁 Brull fell after 1 round."
        );
    }

    #[test]
    fn outro_other_outcomes() {
        assert_eq!(
            outro(&OutcomeType::Remain(0.0), &["A"], &["B", "C"], 2),
            "🏁 B and C fled; A holds the field after 2 rounds."
        );
        assert_eq!(
            outro(&OutcomeType::Remain(0.0), &["A", "D"], &["B"], 1),
            "🏁 B fled; A and D hold the field after 1 round."
        );
        assert_eq!(
            outro(&OutcomeType::Retreat(0.0), &["A"], &["B"], 2),
            "🏁 A escaped after 2 rounds."
        );
        assert_eq!(
            outro(&OutcomeType::Unknown(0.0), &["A"], &["B"], 1),
            "🏁 Undecided after 1 round — the tactician could not see further."
        );
    }
}
