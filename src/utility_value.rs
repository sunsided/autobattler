use crate::conflict::Conflict;
use crate::value::TerminalState;

/// Gets the utility of the current node
pub fn get_utility(state: &Conflict) -> TerminalState {
    if state.initiator.is_defeated() {
        // The current party being dead is a terminal state and always is a negative reward.
        // We sum up the total damage taken to punish strong defeats
        // harder than slight defeats.
        let utility = -state
            .initiator
            .members
            .iter()
            .map(|m| m.damage_taken)
            .sum::<f32>();
        debug_assert!(utility <= 0.0);
        return TerminalState::Defeat(utility);
    }

    if state.initiator.is_gone() {
        // No member is left on the field, and not all are dead, so the party retreated.
        // The penalty is a tenth of the damage taken, so retreating stays
        // preferable to a defeat.
        let utility = -state
            .initiator
            .members
            .iter()
            .map(|m| m.damage_taken)
            .sum::<f32>();
        debug_assert!(utility <= 0.0);
        return TerminalState::Retreat(utility * 0.1);
    }

    // As a naive choice, we simply sum up the health of each member.
    // This is to ensure we play less risky and don't need to heal as much.
    // Health can never be negative, but to be sure we cap it at zero.
    // Members that fled still count, but only at half of their health,
    // since they are no longer fighting but also not dead.
    //
    // In theory we can also factor in the damage taken by the opponent
    // as dealing more damage could be useful. Whether or not that is a
    // useful idea depends on the remaining game mechanics (say, e.g., a massive
    // magical effect that takes a day to recover vs. death by a slap with a stick).
    let utility: f32 = state
        .initiator
        .members
        .iter()
        .map(|m| {
            if m.on_field() {
                m.health.max(0.0)
            } else if m.fled && !m.is_dead() {
                0.5 * m.health.max(0.0)
            } else {
                0.0
            }
        })
        .sum();
    // The initiator is not gone, so at least one member is on the field with positive health.
    debug_assert!(utility > 0.0);

    if state.opponent.is_defeated() {
        TerminalState::Win(utility)
    } else if state.opponent.is_gone() {
        // This is a somewhat delicate balancing. If the utility
        // value for a remain is equal to a win, the opposing party
        // changes their preferences.
        TerminalState::Remain(utility * 0.1)
    } else {
        TerminalState::Heuristic(utility * 0.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::party::Party;
    use crate::party_member::{Morale, PartyMember};
    use crate::weapon::{Stick, Weapon};

    fn member(id: usize, health: f32, damage_taken: f32, fled: bool) -> PartyMember {
        PartyMember {
            id,
            health,
            damage_taken,
            weapon: Weapon::Stick(Stick { damage: 1.0 }),
            can_act: true,
            speed: 1,
            morale: Morale::Tactical,
            fled,
        }
    }

    fn party(id: usize, members: Vec<PartyMember>) -> Party {
        Party {
            id,
            members,
            can_retreat: true,
        }
    }

    fn conflict(heroes: Vec<PartyMember>, villains: Vec<PartyMember>) -> Conflict {
        Conflict::new(party(0, heroes), party(1, villains))
    }

    /// `TerminalState`'s `PartialEq` only compares the payload, so check the variant too.
    fn assert_terminal(actual: TerminalState, expected: TerminalState) {
        assert_eq!(
            std::mem::discriminant(&actual),
            std::mem::discriminant(&expected),
            "expected {expected}, got {actual}"
        );
        assert_eq!(
            actual.value(),
            expected.value(),
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn fled_heroes_count_half() {
        let state = conflict(
            vec![member(0, 20.0, 0.0, false), member(1, 10.0, 0.0, true)],
            vec![member(0, 15.0, 0.0, false)],
        );
        assert_terminal(get_utility(&state), TerminalState::Heuristic(2.5));
    }

    #[test]
    fn win_with_a_fled_hero() {
        let state = conflict(
            vec![member(0, 20.0, 0.0, false), member(1, 10.0, 0.0, true)],
            vec![member(0, 0.0, 15.0, false)],
        );
        assert_terminal(get_utility(&state), TerminalState::Win(25.0));
    }

    #[test]
    fn remain_when_villains_gone() {
        let state = conflict(
            vec![member(0, 20.0, 0.0, false)],
            vec![member(0, 0.0, 15.0, false), member(1, 12.0, 0.0, true)],
        );
        assert_terminal(get_utility(&state), TerminalState::Remain(2.0));
    }

    #[test]
    fn retreat_and_defeat_unchanged() {
        let retreat = conflict(
            vec![member(0, 10.0, 5.0, true)],
            vec![member(0, 15.0, 0.0, false)],
        );
        assert_terminal(get_utility(&retreat), TerminalState::Retreat(-0.5));

        let defeat = conflict(
            vec![member(0, 0.0, 30.0, false)],
            vec![member(0, 15.0, 0.0, false)],
        );
        assert_terminal(get_utility(&defeat), TerminalState::Defeat(-30.0));
    }

    #[test]
    fn mixed_dead_and_fled_heroes_retreat() {
        let state = conflict(
            vec![member(0, 0.0, 25.0, false), member(1, 10.0, 5.0, true)],
            vec![member(0, 15.0, 0.0, false)],
        );
        assert_terminal(get_utility(&state), TerminalState::Retreat(-3.0));
    }
}
