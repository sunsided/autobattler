use crate::action::AppliedAction;
use crate::action_iterator::{ActionIterator, TurnOptions};
use crate::conflict::{Conflict, Turn};
use crate::party::{Participant, Party};
use crate::party_member::Morale;
use crate::search::{self, Search, SearchKind};
use crate::value::{Bound, TerminalState};
use std::time::Duration;

pub struct Solver;

/// The most rounds [`Solver::play`] plays before calling the fight undecided.
pub const MAX_ROUNDS: usize = 50;

/// Classifies a value from the initiator's perspective.
pub(crate) fn outcome_type(value: TerminalState) -> OutcomeType {
    match value {
        TerminalState::Win(score) => OutcomeType::Win(score),
        TerminalState::Defeat(score) => OutcomeType::Lose(score),
        TerminalState::Remain(score) => OutcomeType::Remain(score),
        TerminalState::Retreat(score) => OutcomeType::Retreat(score),
        TerminalState::Heuristic(score) => OutcomeType::Unknown(score),
        TerminalState::OpenUnexplored(score) => OutcomeType::Unknown(score),
    }
}

/// Returns the position after the combatant acting in `turn` takes `action`.
///
/// Fleeing and retreating start the parting blows; attacks finish the turn.
/// The action must be one of the legal [`moves`] of `turn`.
pub(crate) fn child_state(state: &Conflict, turn: &Turn, action: &AppliedAction) -> Conflict {
    let actor_is_initiator = is_maximizing(state, turn);
    let (current, opponent) = Solver::parties_of(state, turn);

    match action {
        AppliedAction::Flee | AppliedAction::Retreat => {
            let mut current = current.clone();
            let fled = if *action == AppliedAction::Flee {
                let actor = turn.actor.member_id;
                current.members[actor].fled = true;
                vec![actor]
            } else {
                current.retreat()
            };

            let mut child =
                Solver::rebuild_state(state, actor_is_initiator, current, opponent.clone());
            child.start_parting(turn, turn.actor.party_id, fled);
            child
        }
        AppliedAction::Targeted(targeted) => {
            debug_assert_eq!(targeted.source, turn.actor);
            debug_assert_eq!(targeted.target.party_id, opponent.id);

            // Targets are always on the field, so the attack always applies.
            let mut target = opponent.members[targeted.target.member_id].clone();
            let applied = target.handle_action(&targeted.action);
            debug_assert!(applied);

            let mut opponent = opponent.clone();
            opponent.replace_member(target);

            let mut child =
                Solver::rebuild_state(state, actor_is_initiator, current.clone(), opponent);
            child.finish_turn(turn);
            child
        }
    }
}

/// The legal moves of the combatant acting in `turn`, in the fixed [`ActionIterator`] order.
pub(crate) fn moves(state: &Conflict, turn: &Turn) -> Vec<AppliedAction> {
    Solver::actions_for(state, turn).collect()
}

/// Whether the combatant acting in `turn` plays for the initiator, who maximizes.
pub(crate) fn is_maximizing(state: &Conflict, turn: &Turn) -> bool {
    turn.actor.party_id == state.initiator.id
}

impl Solver {
    /// Predicts the best line of play from `conflict`, in favor of the initiator.
    ///
    /// See [`search::engage`] for what the timeline holds under each kind.
    #[cfg(test)]
    pub fn engage(conflict: &Conflict, search: Search) -> Outcome {
        search::engage(conflict, search)
    }

    /// Plays the conflict to its end, searching anew before every action.
    ///
    /// Stops early, with an unknown outcome, once `max_rounds` rounds have
    /// been played; see [`search::play`].
    pub fn play(conflict: &Conflict, search: Search, max_rounds: usize) -> Outcome {
        search::play(conflict, search, max_rounds)
    }

    /// Splits the parties of `state` into the one of the acting combatant and its opponent.
    fn parties_of<'a>(state: &'a Conflict, turn: &Turn) -> (&'a Party, &'a Party) {
        if turn.actor.party_id == state.initiator.id {
            (&state.initiator, &state.opponent)
        } else {
            (&state.opponent, &state.initiator)
        }
    }

    /// Creates the iterator over all actions of the combatant acting in `turn`.
    ///
    /// Only the acting combatant is enumerated; member IDs equal their indices.
    fn actions_for(state: &Conflict, turn: &Turn) -> ActionIterator {
        let (current, opponent) = Self::parties_of(state, turn);
        let member_id = turn.actor.member_id;
        ActionIterator::new_in(
            current.clone(),
            opponent.clone(),
            member_id..member_id + 1,
            Self::turn_options(state, turn),
        )
    }

    /// Determines which actions the combatant acting in `turn` may take.
    ///
    /// * A parting blow may only target the fighters who just fled.
    /// * A fighter whose morale broke must flee, even if the party may not retreat.
    /// * A `Tactical` fighter of a party that may retreat can attack, flee alone,
    ///   or order the retreat if someone else on the field would follow.
    /// * Everyone else attacks.
    ///
    /// Nobody flees or retreats once the enemy has left the field.
    pub(crate) fn turn_options(state: &Conflict, turn: &Turn) -> TurnOptions {
        let (current, opponent) = Self::parties_of(state, turn);
        let attacks_only = TurnOptions {
            attacks: true,
            targets: None,
            flee: false,
            retreat: false,
        };

        if let Some(parting) = &state.parting {
            return TurnOptions {
                targets: Some(parting.fleeing.clone()),
                ..attacks_only
            };
        }

        let enemy_present = !opponent.is_gone();
        let actor = &current.members[turn.actor.member_id];
        if actor.must_flee(turn.round) {
            return TurnOptions {
                attacks: false,
                flee: enemy_present,
                ..attacks_only
            };
        }

        if actor.morale == Morale::Tactical && current.can_retreat() {
            let followers = current
                .members
                .iter()
                .any(|m| m.id != actor.id && m.on_field() && m.morale != Morale::Never);
            return TurnOptions {
                flee: enemy_present,
                retreat: enemy_present && followers,
                ..attacks_only
            };
        }

        attacks_only
    }

    /// Builds the child state from the (possibly updated) acting and opposing parties,
    /// keeping the round and slot of the parent state.
    ///
    /// ## Arguments
    /// * `parent` - The state the action was applied to.
    /// * `actor_is_initiator` - Whether the `current` party is the initiator.
    /// * `current` - The party of the acting combatant.
    /// * `opponent` - The other party.
    fn rebuild_state(
        parent: &Conflict,
        actor_is_initiator: bool,
        current: Party,
        opponent: Party,
    ) -> Conflict {
        let (initiator, opponent) = if actor_is_initiator {
            (current, opponent)
        } else {
            (opponent, current)
        };
        Conflict {
            initiator,
            opponent,
            round: parent.round,
            slot: parent.slot,
            parting: parent.parting.clone(),
        }
    }
}

/// An outcome of a conflict.
pub struct Outcome {
    /// Whether the initiating party wins the conflict.
    pub outcome: OutcomeType,
    /// An optimal path of actions leading to the outcome.
    pub timeline: Vec<Event>,
    /// The number of node evaluations performed.
    pub evaluations: usize,
    /// The number of pruning steps performed.
    pub cuts: usize,
    /// The search duration.
    pub search_duration: Duration,
    /// The depth of the deepest node evaluated.
    pub max_visited_depth: usize,
    /// `true` if the search was depth limited and has more nodes to explore.
    pub depth_limited: bool,
    /// The search that found the outcome.
    pub search: SearchKind,
}

impl Outcome {
    /// The number of events in the timeline.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.timeline.len()
    }
}

/// The type of outcome.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum OutcomeType {
    /// The initiating party wins.
    Win(f32),
    /// The initiating party loses.
    Lose(f32),
    /// The initiating party remained after the opponent retreated.
    Remain(f32),
    /// The initiating party retreated.
    Retreat(f32),
    /// Unknown outcome.
    Unknown(f32),
}

/// An event in the timeline.
pub struct Event {
    /// The round in which the event took place, starting at 1.
    pub round: usize,
    /// The slot in the initiative order of the acting combatant.
    pub slot: usize,
    /// The acting combatant.
    pub actor: Participant,
    /// The action that was applied.
    pub action: AppliedAction,
    /// The state of the conflict after the action took place.
    pub state: Conflict,
    /// The search score of the action, from the initiator's perspective.
    pub score: TerminalState,
    /// Whether [`score`](Event::score) is exact or a bound due to pruning.
    pub bound: Bound,
    /// The other explored actions of the same combatant, best first from its perspective.
    pub alternatives: Vec<Alternative>,
    /// The number of legal actions that were never generated due to pruning.
    pub unexamined: usize,
}

/// An action the acting combatant could have taken instead.
#[derive(Debug, Clone)]
pub struct Alternative {
    /// The action.
    pub action: AppliedAction,
    /// The search score of the action, from the initiator's perspective.
    pub score: TerminalState,
    /// Whether [`score`](Alternative::score) is exact or a bound due to pruning.
    pub bound: Bound,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, TargetedAction};
    use crate::action_iterator::TurnOptions;
    use crate::cli::Options;
    use crate::party_member::{Morale, PartyMember};
    use crate::weapon::{Fists, Stick, Weapon};

    /// The search the tests were written against: `basic`, deepening to `depth`.
    fn basic(depth: usize) -> Search {
        Search {
            kind: SearchKind::Basic,
            depth,
        }
    }

    /// The search the CLI and the TUI use unless told otherwise.
    fn default_kind(depth: usize) -> Search {
        Search {
            kind: Options::default().search,
            depth,
        }
    }

    #[test]
    fn simple_fight_works() {
        // The hero is faster, so this is identical to strict alternation.
        let heroes = build_default_hero_party(false, 25.0, 6);
        let villains = build_simple_villain_party();

        let conflict = Conflict::new(heroes, villains);

        for search in [basic(200), default_kind(200)] {
            let solution = Solver::engage(&conflict, search);
            assert_eq!(solution.outcome, OutcomeType::Win(5.0), "{search:?}");
        }
    }

    #[test]
    fn complex_fight_works() {
        let conflict = complex_works_conflict();

        // In this version, the enemy is not allowed to flee. The hero kills
        // Molphige first, then trades blows with Ziuon.
        let solution = Solver::engage(&conflict, basic(100));
        assert_eq!(solution.outcome, OutcomeType::Win(10.0));
        assert_eq!(solution.len(), 5);

        let solution = Solver::engage(&conflict, default_kind(100));
        assert_eq!(solution.outcome, OutcomeType::Win(10.0));
    }

    #[test]
    fn complex_fight_opponent_flees() {
        let conflict = complex_flees_conflict();

        // The hero kills Molphige. Fleeing later would let the hero's parting
        // blow finish Ziuon off, so he flees at once and survives it with 5
        // health: 20 health * 0.1. With Molphige dead, Ziuon has nobody to
        // order out, so he flees alone rather than retreating.
        let solution = Solver::engage(&conflict, basic(100));
        assert_eq!(solution.outcome, OutcomeType::Remain(2.0));
        assert_eq!(solution.len(), 3);
        assert_eq!(solution.timeline[1].action, AppliedAction::Flee);

        let solution = Solver::engage(&conflict, default_kind(100));
        assert_eq!(solution.outcome, OutcomeType::Remain(2.0));
    }

    #[test]
    fn complex_fight_initiator_cannot_outrun_faster_enemy() {
        let heroes = build_default_hero_party(true, 20.0, 6); // 👈 an opponent exists that does equal damage
        let villains = build_complex_villain_party(true, 15.0); // 👈 more health than hero does damage

        let conflict = Conflict::new(heroes, villains);

        let solution = Solver::engage(&conflict, basic(100));

        // Fleeing in slot 0 still lets Molphige (slot 1) strike, which
        // defeats the hero in one hit. The villains' new ways out come after
        // their attacks and are never generated: the first punch is lethal.
        assert_eq!(solution.outcome, OutcomeType::Lose(-20.0));
        assert_eq!(solution.len(), 2);

        let solution = Solver::engage(&conflict, default_kind(100));
        assert_eq!(solution.outcome, OutcomeType::Lose(-20.0));
    }

    #[test]
    fn complex_fight_initiator_defeated() {
        let heroes = build_default_hero_party(false, 20.0, 6); // 👈 an opponent exists that does equal damage
        let villains = build_complex_villain_party(true, 15.0); // 👈 more health than hero does damage

        let conflict = Conflict::new(heroes, villains);

        let solution = Solver::engage(&conflict, basic(100));

        // Any first move is answered by Molphige's 20-damage punch.
        assert_eq!(solution.outcome, OutcomeType::Lose(-20.0));
        assert_eq!(solution.len(), 2);

        let solution = Solver::engage(&conflict, default_kind(100));
        assert_eq!(solution.outcome, OutcomeType::Lose(-20.0));
    }

    #[test]
    fn slow_initiator_pays_parting_blows_when_fleeing() {
        // Order: Molphige(5), Ziuon(3), hero(1). Hero takes 20 + 5, flees from
        // the last slot, and both villains strike again: Retreat(-50 * 0.1).
        // Fleeing villains would hand the hero a Remain, so they keep attacking;
        // their extra options can only lower the fighting line, which already loses.
        let conflict = Conflict::new(
            build_default_hero_party(true, 60.0, 1),
            build_complex_villain_party(true, 15.0),
        );
        let solution = Solver::engage(&conflict, basic(100));
        assert_eq!(solution.outcome, OutcomeType::Retreat(-5.0));
        assert_eq!(solution.len(), 5);
        assert_eq!(solution.timeline[2].action, AppliedAction::Flee);
        assert_eq!(
            solution.timeline[0].actor,
            Participant {
                party_id: 1,
                member_id: 1
            }
        );

        let solution = Solver::engage(&conflict, default_kind(100));
        assert_eq!(solution.outcome, OutcomeType::Retreat(-5.0));
    }

    #[test]
    fn events_carry_round_slot_and_actor() {
        let conflict = Conflict::new(
            build_default_hero_party(true, 20.0, 6),
            build_complex_villain_party(false, 10.0),
        );
        let solution = Solver::engage(&conflict, basic(100));
        let e = &solution.timeline;
        assert_eq!((e[0].round, e[0].slot), (1, 0)); // Brull
        assert_eq!((e[1].round, e[1].slot), (1, 2)); // Ziuon; Molphige's slot skipped
        assert_eq!((e[2].round, e[2].slot), (2, 0));
        assert_eq!(
            e[1].actor,
            Participant {
                party_id: 1,
                member_id: 0
            }
        );
    }

    #[test]
    fn already_decided_conflict_has_empty_timeline() {
        let mut villains = build_simple_villain_party();
        villains.members[0].health = 0.0;
        let conflict = Conflict::new(build_default_hero_party(false, 20.0, 6), villains);
        for search in [basic(10), default_kind(10)] {
            let solution = Solver::engage(&conflict, search);
            assert!(solution.timeline.is_empty(), "{search:?}");
            assert!(
                matches!(solution.outcome, OutcomeType::Win(_)),
                "{search:?}"
            );
        }
    }

    fn simple_conflict() -> Conflict {
        Conflict::new(
            build_default_hero_party(false, 25.0, 6),
            build_simple_villain_party(),
        )
    }

    #[test]
    fn play_continues_past_the_lookahead() {
        let conflict = simple_conflict();
        for search in [basic(2), default_kind(2)] {
            assert!(
                matches!(
                    Solver::engage(&conflict, search).outcome,
                    OutcomeType::Unknown(_)
                ),
                "{search:?}"
            );

            let played = Solver::play(&conflict, search, MAX_ROUNDS);
            assert_eq!(played.outcome, OutcomeType::Win(5.0), "{search:?}");
            let end = &played.timeline.last().expect("events").state;
            assert!(end.next_turn().is_none(), "{search:?}");
            assert_eq!(played.timeline.len(), 5, "{search:?}");
        }
    }

    #[test]
    fn play_matches_a_full_search_when_it_sees_the_end() {
        let conflict = complex_works_conflict();
        let searched = Solver::engage(&conflict, basic(100));
        let played = Solver::play(&conflict, basic(100), MAX_ROUNDS);
        assert_eq!(played.outcome, searched.outcome);
        assert_eq!(played.timeline.len(), searched.timeline.len());
        assert!(played.evaluations >= searched.evaluations);

        let searched = Solver::engage(&conflict, default_kind(100));
        let played = Solver::play(&conflict, default_kind(100), MAX_ROUNDS);
        assert_eq!(played.outcome, searched.outcome);
    }

    #[test]
    fn play_stops_at_the_round_limit() {
        for search in [basic(2), default_kind(2)] {
            let played = Solver::play(&simple_conflict(), search, 1);
            assert!(
                matches!(played.outcome, OutcomeType::Unknown(_)),
                "{search:?}"
            );
            assert!(played.timeline.iter().all(|e| e.round == 1), "{search:?}");
            assert_eq!(played.timeline.len(), 2, "{search:?}");
        }
    }

    #[test]
    fn play_of_a_decided_conflict_is_empty() {
        let mut villains = build_simple_villain_party();
        villains.members[0].health = 0.0;
        let conflict = Conflict::new(build_default_hero_party(false, 20.0, 6), villains);
        for search in [basic(10), default_kind(10)] {
            let played = Solver::play(&conflict, search, MAX_ROUNDS);
            assert!(played.timeline.is_empty(), "{search:?}");
            assert!(matches!(played.outcome, OutcomeType::Win(_)), "{search:?}");
        }
    }

    #[test]
    fn play_names_its_search() {
        for search in [basic(2), default_kind(2)] {
            let played = Solver::play(&simple_conflict(), search, MAX_ROUNDS);
            assert_eq!(played.search, search.kind);
        }
    }

    #[test]
    fn principal_variation_is_exact_when_fight_is_won() {
        let solution = Solver::engage(&complex_works_conflict(), basic(100));
        assert!(solution.timeline.iter().all(|e| e.bound == Bound::Exact));
        assert_eq!(solution.timeline[0].score, TerminalState::Win(10.0));

        let solution = Solver::engage(&complex_works_conflict(), default_kind(100));
        assert_eq!(solution.timeline[0].score, TerminalState::Win(10.0));
    }

    #[test]
    fn first_move_lists_all_other_options() {
        // Brull: fists/stare x Ziuon/Molphige + flee = 5 legal moves.
        let e = &Solver::engage(&complex_works_conflict(), basic(100)).timeline[0];
        assert_eq!(e.alternatives.len() + e.unexamined, 4);
        // Punching Ziuon lets Molphige punch back: cut off as soon as the defeat is seen.
        assert!(e
            .alternatives
            .iter()
            .any(|a| a.bound == Bound::AtMost && a.score == TerminalState::Defeat(-20.0)));
        let punch_ziuon = e
            .alternatives
            .iter()
            .find(|a| matches!(&a.action, AppliedAction::Targeted(t) if is_weapon_attack(t, 1, 0)))
            .expect("Punching Ziuon is an alternative");
        assert_eq!(punch_ziuon.bound, Bound::AtMost);
        assert_eq!(punch_ziuon.score, TerminalState::Defeat(-20.0));

        let e = &Solver::engage(&complex_works_conflict(), default_kind(100)).timeline[0];
        assert_eq!(e.alternatives.len() + e.unexamined, 4);
    }

    #[test]
    fn bounds_propagate_through_fully_explored_nodes() {
        // The hero cannot win: every round-2 hero move is cut off at Molphige's lethal
        // reply (AtMost). Ziuon's round-1 hit leads to a maximizing position that
        // tries all of those moves without a cutoff itself, so its score is AtMost too,
        // and so is the event of Ziuon's hit on the best line.
        let conflict = Conflict::new(
            build_default_hero_party(false, 30.0, 6),
            build_complex_villain_party(false, 15.0),
        );
        let solution = Solver::engage(&conflict, basic(100));
        let e = &solution.timeline[2];
        assert_eq!(
            e.actor,
            Participant {
                party_id: 1,
                member_id: 0
            }
        );
        assert_eq!(e.score, TerminalState::Defeat(-30.0));
        assert_eq!(e.bound, Bound::AtMost);

        // Searched from the position before Ziuon's hit, the same move is the
        // root's choice; every other move it tried is bounded the same way.
        let before = &solution.timeline[1].state;
        let solution = Solver::engage(before, basic(100));
        let e = &solution.timeline[0];
        assert_eq!(e.actor, participant(1, 0));
        assert_eq!(e.score, TerminalState::Defeat(-30.0));
        assert_eq!(e.bound, Bound::AtMost);
        assert!(e.alternatives.iter().all(|a| a.bound == Bound::AtMost));
    }

    fn participant(party_id: usize, member_id: usize) -> Participant {
        Participant {
            party_id,
            member_id,
        }
    }

    fn is_weapon_attack(action: &TargetedAction, party_id: usize, member_id: usize) -> bool {
        let Action::SimpleAttack(attack) = &action.action;
        attack.weapon.is_some()
            && action.source
                == Participant {
                    party_id: 0,
                    member_id: 0,
                }
            && action.target
                == Participant {
                    party_id,
                    member_id,
                }
    }

    #[test]
    fn alternatives_are_ranked_from_the_movers_perspective() {
        for conflict in [complex_works_conflict(), complex_flees_conflict()] {
            for search in [basic(100), default_kind(100)] {
                let solution = Solver::engage(&conflict, search);
                for e in &solution.timeline {
                    let hero = e.actor.party_id == conflict.initiator.id;
                    for pair in e.alternatives.windows(2) {
                        if hero {
                            assert!(pair[0].score >= pair[1].score)
                        } else {
                            assert!(pair[0].score <= pair[1].score)
                        }
                    }
                    for a in &e.alternatives {
                        if hero {
                            assert!(a.score <= e.score)
                        } else {
                            assert!(a.score >= e.score)
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn forced_defeat_reports_bounds_and_unexamined_moves() {
        // Every hero move is answered by a lethal punch; the minimizer stops at the first
        // defeat it sees, so the chosen line is itself a bound.
        let conflict = Conflict::new(
            build_default_hero_party(false, 20.0, 6),
            build_complex_villain_party(true, 15.0),
        );
        let solution = Solver::engage(&conflict, basic(100));
        assert_eq!(solution.timeline[0].bound, Bound::AtMost);

        // Only the first event lists what was tried, so search Molphige's turn
        // as the root. He may punch or stare at the hero, flee alone, or order a
        // retreat Ziuon would follow (both are Tactical and still on the field):
        // four moves, only the punch was generated. The punch ends the fight,
        // so its own score is exact.
        let after = &solution.timeline[0].state;
        for search in [basic(100), default_kind(100)] {
            let solution = Solver::engage(after, search);
            assert_eq!(solution.outcome, OutcomeType::Lose(-20.0), "{search:?}");
            let e = &solution.timeline[0];
            assert_eq!(e.actor, participant(1, 1), "{search:?}");
            assert_eq!(e.score, TerminalState::Defeat(-20.0), "{search:?}");
            assert_eq!(e.bound, Bound::Exact, "{search:?}");
            assert!(e.alternatives.is_empty(), "{search:?}");
            assert_eq!(e.unexamined, 3, "{search:?}");
        }
    }

    fn fighter(id: usize, morale: Morale) -> PartyMember {
        PartyMember {
            id,
            health: 20.0,
            damage_taken: 0.0,
            weapon: Weapon::Fists(Fists { damage: 10.0 }),
            can_act: true,
            speed: 6 - id as u32,
            morale,
            fled: false,
        }
    }

    /// Options of hero member 0 in round 1, heroes given by morale.
    fn hero_options(morales: &[Morale], can_retreat: bool) -> TurnOptions {
        let heroes = Party {
            id: 0,
            members: morales
                .iter()
                .enumerate()
                .map(|(id, &morale)| fighter(id, morale))
                .collect(),
            can_retreat,
        };
        let conflict = Conflict::new(heroes, build_simple_villain_party());
        let turn = conflict.next_turn().expect("a turn");
        assert_eq!(
            turn.actor,
            Participant {
                party_id: 0,
                member_id: 0
            }
        );
        Solver::turn_options(&conflict, &turn)
    }

    fn options(attacks: bool, flee: bool, retreat: bool) -> TurnOptions {
        TurnOptions {
            attacks,
            targets: None,
            flee,
            retreat,
        }
    }

    #[test]
    fn never_only_attacks() {
        assert_eq!(
            hero_options(&[Morale::Never, Morale::Tactical], true),
            options(true, false, false)
        );
    }

    #[test]
    fn broken_courage_only_flees_even_without_retreat() {
        assert_eq!(
            hero_options(&[Morale::Courage(1.0)], false),
            options(false, true, false)
        );
    }

    #[test]
    fn tactical_alone_cannot_order_a_retreat() {
        assert_eq!(
            hero_options(&[Morale::Tactical], true),
            options(true, true, false)
        );
    }

    #[test]
    fn tactical_with_only_a_never_teammate_cannot_order_a_retreat() {
        assert_eq!(
            hero_options(&[Morale::Tactical, Morale::Never], true),
            options(true, true, false)
        );
    }

    #[test]
    fn tactical_with_a_tactical_teammate_may_order_a_retreat() {
        assert_eq!(
            hero_options(&[Morale::Tactical, Morale::Tactical], true),
            options(true, true, true)
        );
    }

    #[test]
    fn tactical_without_retreat_only_attacks() {
        assert_eq!(
            hero_options(&[Morale::Tactical, Morale::Tactical], false),
            options(true, false, false)
        );
    }

    #[test]
    fn parting_turn_only_attacks_the_fleeing() {
        // Both villains are Tactical and may retreat, but a parting turn
        // offers neither way out: only blows at the fighter who fled.
        let mut conflict = complex_flees_conflict();
        let order = conflict.initiative_order();
        let fled = Turn {
            round: 1,
            slot: 1,
            actor: order[1].clone(),
        };
        assert_eq!(
            fled.actor,
            Participant {
                party_id: 1,
                member_id: 1
            }
        );
        conflict.opponent.members[1].fled = true;
        conflict.start_parting(&fled, 1, vec![1]);

        let turn = conflict.next_turn().expect("a parting blow");
        assert_eq!(
            turn.actor,
            Participant {
                party_id: 0,
                member_id: 0
            }
        );
        let options = Solver::turn_options(&conflict, &turn);
        assert_eq!(
            options,
            TurnOptions {
                attacks: true,
                targets: Some(vec![1]),
                flee: false,
                retreat: false,
            }
        );
        assert!(!options.flee && !options.retreat);
    }

    #[test]
    fn cowardly_villain_flees_on_first_turn() {
        // The hero (speed 6) hits for 10: the villain is at 15. Its courage of
        // 1.0 is broken from the start, so fleeing is its only option. The
        // hero's parting blow brings it to 5, then the villains are gone and
        // the hero remains with all 20 health: 20 * 0.1.
        let heroes = build_default_hero_party(false, 20.0, 6);
        let mut villains = build_simple_villain_party();
        villains.members[0].morale = Morale::Courage(1.0);
        let conflict = Conflict::new(heroes, villains);

        let solution = Solver::engage(&conflict, default_kind(100));
        assert_eq!(solution.outcome, OutcomeType::Remain(2.0));

        let solution = Solver::engage(&conflict, basic(100));
        assert_eq!(solution.outcome, OutcomeType::Remain(2.0));
        let actions: Vec<_> = solution.timeline.iter().map(|e| &e.action).collect();
        assert!(matches!(
            actions[..],
            [
                AppliedAction::Targeted(_),
                AppliedAction::Flee,
                AppliedAction::Targeted(_)
            ]
        ));
        // The utility ignores opponent health, so several moves tie at Remain(2.0)
        // and the search keeps the first one generated (the full attack), which is
        // why the villain ends at 5 health.
        assert_eq!(solution.timeline[2].state.opponent.members[0].health, 5.0);
    }

    fn complex_works_conflict() -> Conflict {
        let heroes = build_default_hero_party(true, 20.0, 6); // 👈 an opponent exists that does equal damage
        let villains = build_complex_villain_party(false, 10.0);
        Conflict::new(heroes, villains)
    }

    fn complex_flees_conflict() -> Conflict {
        let heroes = build_default_hero_party(true, 20.0, 6); // 👈 an opponent exists that does equal damage
        let villains = build_complex_villain_party(true, 10.0); // 👈 equal health to the hero damage
        Conflict::new(heroes, villains)
    }

    fn build_default_hero_party(can_retreat: bool, health: f32, speed: u32) -> Party {
        Party {
            id: 0,
            members: vec![PartyMember {
                id: 0,
                health,
                damage_taken: 0.0,
                weapon: Weapon::Fists(Fists { damage: 10.0 }),
                can_act: true,
                speed,
                morale: Morale::Tactical,
                fled: false,
            }],
            can_retreat,
        }
    }

    fn build_complex_villain_party(can_retreat: bool, dangerous_health: f32) -> Party {
        Party {
            id: 1,
            members: vec![
                PartyMember {
                    id: 0,
                    health: 15.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Stick(Stick { damage: 5.0 }),
                    can_act: true,
                    speed: 3,
                    morale: Morale::Tactical,
                    fled: false,
                },
                PartyMember {
                    id: 1,
                    health: dangerous_health,
                    damage_taken: 0.0,
                    weapon: Weapon::Fists(Fists {
                        damage: 20.0, // 👈 may defeat hero in one hit
                    }),
                    can_act: true,
                    speed: 5,
                    morale: Morale::Tactical,
                    fled: false,
                },
            ],
            can_retreat,
        }
    }

    fn build_simple_villain_party() -> Party {
        Party {
            id: 1,
            members: vec![PartyMember {
                id: 0,
                health: 25.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 10.0 }),
                can_act: true,
                speed: 3,
                morale: Morale::Tactical,
                fled: false,
            }],
            can_retreat: false,
        }
    }

    #[test]
    fn same_value_works() {
        assert_eq!(f32::INFINITY, f32::INFINITY);
        assert_eq!(f32::NEG_INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::NEG_INFINITY, f32::INFINITY);
    }
}
