use crate::conflict::Conflict;
use crate::value::TerminalState;

/// Gets the utility of the current node
pub fn get_utility(state: &Conflict) -> TerminalState {
    if state.initiator.is_defeated() || state.initiator.has_retreated() {
        // The current party being dead is a terminal state and always is a negative reward.
        // We sum up the total damage taken to punish strong defeats
        // harder than slight defeats.
        let utility = state
            .initiator
            .members
            .iter()
            .map(|m| -m.damage_taken)
            .sum();
        debug_assert!(utility < 0.0);
        return if state.initiator.is_defeated() {
            TerminalState::Defeat(utility)
        } else {
            TerminalState::Retreat(utility * 0.1)
        };
    }

    // As a naive choice, we simply sum up the health of each member.
    // This is to ensure we play less risky and don't need to heal as much.
    // Health can never be negative, but to be sure we cap it at zero.
    //
    // In theory we can also factor in the damage taken by the opponent
    // as dealing more damage could be useful. Whether or not that is a
    // useful idea depends on the remaining game mechanics (say, e.g., a massive
    // magical effect that takes a day to recover vs. death by a slap with a stick).
    let utility = state
        .initiator
        .members
        .iter()
        .map(|m| m.health.max(0.0))
        .sum();
    debug_assert!(utility > 0.0);

    if state.opponent.is_defeated() {
        TerminalState::Win(utility)
    } else if state.opponent.has_retreated() {
        // This is a somewhat delicate balancing. If the utility
        // value for a remain is equal to a win, the opposing party
        // changes their preferences.
        TerminalState::Remain(utility * 0.1)
    } else {
        TerminalState::Heuristic(utility * 0.1)
    }
}
