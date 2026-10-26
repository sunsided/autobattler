use crate::party::{Participant, Party};
use crate::party_member::PartyMember;

/// A conflict, specifically the state of conflict at a given turn.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The party initiating the conflict, the maximizing player.
    pub initiator: Party,
    /// The other involved party, the minimizing player.
    pub opponent: Party,
    /// The current round, starting at 1.
    pub round: usize,
    /// The index into the [`initiative order`](Conflict::initiative_order)
    /// at which the next turn of the current round is searched.
    pub slot: usize,
}

/// A single turn of a single combatant in a round.
#[allow(dead_code)] // Wired into the solver in a later step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// The round the turn takes place in.
    pub round: usize,
    /// The slot in the initiative order the actor occupies.
    pub slot: usize,
    /// The acting combatant.
    pub actor: Participant,
}

#[allow(dead_code)] // Initiative API is wired into the solver in a later step.
impl Conflict {
    /// Creates a new conflict at the start of the first round.
    pub fn new(initiator: Party, opponent: Party) -> Self {
        Self {
            initiator,
            opponent,
            round: 1,
            slot: 0,
        }
    }

    /// Returns all members (dead ones included) ordered by initiative.
    ///
    /// Members are sorted by speed (descending), then the initiator party first,
    /// then by member ID. Dead members are kept so that slot indices are stable
    /// over the course of a round.
    pub fn initiative_order(&self) -> Vec<Participant> {
        let mut order: Vec<(u32, bool, Participant)> =
            [(&self.initiator, true), (&self.opponent, false)]
                .into_iter()
                .flat_map(|(party, is_initiator)| {
                    party.members.iter().map(move |m| {
                        (
                            m.speed,
                            is_initiator,
                            Participant {
                                party_id: party.id,
                                member_id: m.id,
                            },
                        )
                    })
                })
                .collect();
        order.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.1.cmp(&a.1))
                .then(a.2.member_id.cmp(&b.2.member_id))
        });
        order.into_iter().map(|(_, _, p)| p).collect()
    }

    /// Determines the next turn, if any.
    ///
    /// Returns `None` if either party is defeated, or if nobody is left to act.
    /// If a party retreated, the current round is finished but no new round starts.
    pub fn next_turn(&self) -> Option<Turn> {
        if self.initiator.is_defeated() || self.opponent.is_defeated() {
            return None;
        }

        let order = self.initiative_order();
        if let Some(turn) = self.find_actor(&order, self.round, self.slot) {
            return Some(turn);
        }

        if self.initiator.has_retreated() || self.opponent.has_retreated() {
            return None;
        }

        self.find_actor(&order, self.round + 1, 0)
    }

    /// Marks the turn as finished, advancing to the next slot.
    pub fn finish_turn(&mut self, turn: &Turn) {
        self.round = turn.round;
        self.slot = turn.slot + 1;
    }

    fn find_actor(&self, order: &[Participant], round: usize, from_slot: usize) -> Option<Turn> {
        order
            .iter()
            .enumerate()
            .skip(from_slot)
            .find(|(_, p)| self.targeted_member(p).can_act())
            .map(|(slot, p)| Turn {
                round,
                slot,
                actor: p.clone(),
            })
    }

    /// Selects the action target by ID.
    pub fn action_target(&self, party_id: usize) -> &Party {
        if self.initiator.id == party_id {
            &self.initiator
        } else {
            debug_assert_eq!(self.opponent.id, party_id);
            &self.opponent
        }
    }

    /// Selects the action target by ID.
    pub fn targeted_member(&self, target: &Participant) -> &PartyMember {
        let party = self.action_target(target.party_id);
        &party.members[target.member_id]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::{Fists, Stick, Weapon};

    fn p(party_id: usize, member_id: usize) -> Participant {
        Participant {
            party_id,
            member_id,
        }
    }

    fn member(id: usize, speed: u32, health: f32, weapon: Weapon) -> PartyMember {
        PartyMember {
            id,
            health,
            damage_taken: 0.0,
            weapon,
            can_act: true,
            speed,
        }
    }

    fn scenario(hero_speed: u32) -> Conflict {
        let heroes = Party {
            id: 0,
            members: vec![member(
                0,
                hero_speed,
                20.0,
                Weapon::Fists(Fists { damage: 10.0 }),
            )],
            can_retreat: true,
            retreated: false,
        };
        let villains = Party {
            id: 1,
            members: vec![
                member(0, 3, 15.0, Weapon::Stick(Stick { damage: 5.0 })),
                member(1, 5, 10.0, Weapon::Fists(Fists { damage: 20.0 })),
            ],
            can_retreat: true,
            retreated: false,
        };
        Conflict::new(heroes, villains)
    }

    #[test]
    fn initiative_orders_by_speed() {
        assert_eq!(
            scenario(6).initiative_order(),
            vec![p(0, 0), p(1, 1), p(1, 0)]
        );
    }

    #[test]
    fn initiative_ties_prefer_initiator_then_lower_id() {
        let mut c = scenario(4);
        c.opponent.members[0].speed = 4;
        c.opponent.members[1].speed = 4;
        assert_eq!(c.initiative_order(), vec![p(0, 0), p(1, 0), p(1, 1)]);
    }

    #[test]
    fn first_turn_is_fastest_combatant() {
        assert_eq!(
            scenario(6).next_turn(),
            Some(Turn {
                round: 1,
                slot: 0,
                actor: p(0, 0)
            })
        );
    }

    #[test]
    fn dead_combatant_is_skipped_mid_round() {
        let mut c = scenario(6);
        let t = c.next_turn().unwrap();
        c.opponent.members[1].health = 0.0; // Molphige dies to the hero's action
        c.finish_turn(&t);
        assert_eq!(
            c.next_turn(),
            Some(Turn {
                round: 1,
                slot: 2,
                actor: p(1, 0)
            })
        );
    }

    #[test]
    fn round_wraps_after_last_slot() {
        let mut c = scenario(6);
        c.finish_turn(&Turn {
            round: 1,
            slot: 2,
            actor: p(1, 0),
        });
        assert_eq!(
            c.next_turn(),
            Some(Turn {
                round: 2,
                slot: 0,
                actor: p(0, 0)
            })
        );
    }

    #[test]
    fn retreat_lets_enemy_finish_the_round_then_ends() {
        let mut c = scenario(6);
        let t = c.next_turn().unwrap();
        c.initiator.retreat();
        c.finish_turn(&t);
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(1, 1));
        c.finish_turn(&t);
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(1, 0));
        c.finish_turn(&t);
        assert_eq!(c.next_turn(), None);
    }

    #[test]
    fn retreat_in_last_slot_gives_no_parting_action() {
        let mut c = scenario(6);
        c.finish_turn(&Turn {
            round: 1,
            slot: 1,
            actor: p(1, 1),
        });
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(1, 0));
        c.opponent.retreat();
        c.finish_turn(&t);
        assert_eq!(c.next_turn(), None);
    }

    #[test]
    fn next_turn_is_none_when_nobody_can_act() {
        let mut c = scenario(6);
        for m in c
            .initiator
            .members
            .iter_mut()
            .chain(c.opponent.members.iter_mut())
        {
            m.can_act = false;
        }
        assert_eq!(c.next_turn(), None);
    }

    #[test]
    fn next_turn_is_none_when_a_party_is_defeated() {
        let mut c = scenario(6);
        c.opponent.members.iter_mut().for_each(|m| m.health = 0.0);
        assert_eq!(c.next_turn(), None);
    }
}
