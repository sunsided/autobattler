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
    /// Set while fighters have just fled: every enemy who can act gets one
    /// parting blow before the interrupted round resumes.
    pub parting: Option<Parting>,
}

/// The parting blows that interrupt a round after fighters fled.
#[derive(Debug, Clone, PartialEq)]
pub struct Parting {
    /// The party the fleeing fighters belong to.
    pub fleeing_party: usize,
    /// The IDs of the fighters who fled.
    pub fleeing: Vec<usize>,
    /// The slot at which the interrupted round resumes.
    pub resume_slot: usize,
    /// The initiative index at which the next parting actor is searched.
    pub next: usize,
}

/// A single turn of a single combatant in a round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// The round the turn takes place in.
    pub round: usize,
    /// The slot in the initiative order the actor occupies.
    pub slot: usize,
    /// The acting combatant.
    pub actor: Participant,
}

impl Conflict {
    /// Creates a new conflict at the start of the first round.
    pub fn new(initiator: Party, opponent: Party) -> Self {
        Self {
            initiator,
            opponent,
            round: 1,
            slot: 0,
            parting: None,
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
    /// Returns `None` if either party is defeated. While parting blows are
    /// pending, the next one is returned. Otherwise returns `None` if either
    /// party has left the field, or the next actor of this or the next round.
    pub fn next_turn(&self) -> Option<Turn> {
        if self.initiator.is_defeated() || self.opponent.is_defeated() {
            return None;
        }

        let order = self.initiative_order();
        if self.parting.is_some() {
            return self.parting_actor(&order);
        }

        if self.initiator.is_gone() || self.opponent.is_gone() {
            return None;
        }

        if let Some(turn) = self.find_actor(&order, self.round, self.slot) {
            return Some(turn);
        }

        self.find_actor(&order, self.round + 1, 0)
    }

    /// Marks the turn as finished, advancing to the next slot, or to the next
    /// parting blow while parting.
    pub fn finish_turn(&mut self, turn: &Turn) {
        if let Some(parting) = &mut self.parting {
            parting.next = turn.slot + 1;
            self.settle_parting();
        } else {
            self.round = turn.round;
            self.slot = turn.slot + 1;
        }
    }

    /// Interrupts the round after `fleeing` fighters of `fleeing_party` fled
    /// in `turn`: every enemy who can act strikes once, then the round resumes.
    pub fn start_parting(&mut self, turn: &Turn, fleeing_party: usize, fleeing: Vec<usize>) {
        debug_assert!(
            self.parting.is_none(),
            "Parting blows cannot be interrupted by another flight"
        );
        self.round = turn.round;
        self.parting = Some(Parting {
            fleeing_party,
            fleeing,
            resume_slot: turn.slot + 1,
            next: 0,
        });
        self.settle_parting();
    }

    /// Ends the parting phase once no parting actor remains.
    fn settle_parting(&mut self) {
        let Some(parting) = &self.parting else {
            return;
        };
        if self.parting_actor(&self.initiative_order()).is_none() {
            self.slot = parting.resume_slot;
            self.parting = None;
        }
    }

    /// Finds the next enemy of the fleeing party who may strike a parting blow.
    fn parting_actor(&self, order: &[Participant]) -> Option<Turn> {
        let parting = self.parting.as_ref()?;
        let fleeing_party = self.action_target(parting.fleeing_party);
        if !parting
            .fleeing
            .iter()
            .any(|&id| !fleeing_party.members[id].is_dead())
        {
            return None;
        }
        order
            .iter()
            .enumerate()
            .skip(parting.next)
            .find(|(_, p)| p.party_id != parting.fleeing_party && self.targeted_member(p).can_act())
            .map(|(slot, p)| Turn {
                round: self.round,
                slot,
                actor: p.clone(),
            })
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
    use crate::party_member::Morale;
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
            morale: Morale::Tactical,
            fled: false,
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
        };
        let villains = Party {
            id: 1,
            members: vec![
                member(0, 3, 15.0, Weapon::Stick(Stick { damage: 5.0 })),
                member(1, 5, 10.0, Weapon::Fists(Fists { damage: 20.0 })),
            ],
            can_retreat: true,
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

    /// Plays `turn`, retreating the actor's party first if `flee` is set.
    fn act(c: &mut Conflict, flee: bool) -> Turn {
        let t = c.next_turn().unwrap();
        if flee {
            let party = t.actor.party_id;
            let fled = if party == c.initiator.id {
                c.initiator.retreat()
            } else {
                c.opponent.retreat()
            };
            c.start_parting(&t, party, fled);
        } else {
            c.finish_turn(&t);
        }
        t
    }

    fn remaining_actors(mut c: Conflict) -> Vec<Participant> {
        let mut actors = Vec::new();
        while c.next_turn().is_some() {
            actors.push(act(&mut c, false).actor);
        }
        actors
    }

    #[test]
    fn retreat_gives_every_enemy_one_parting_blow() {
        let mut c = scenario(6);
        act(&mut c, true);
        assert!(c.parting.is_some());
        assert_eq!(remaining_actors(c), vec![p(1, 1), p(1, 0)]);
    }

    #[test]
    fn parting_blows_include_enemies_who_already_acted() {
        // The hero is slowest: both villains act, then the hero flees.
        let mut c = scenario(1);
        act(&mut c, false);
        act(&mut c, false);
        let t = act(&mut c, true);
        assert_eq!(t.actor, p(0, 0));
        let next = c.next_turn().unwrap();
        assert_eq!((next.round, next.slot), (1, 0));
        assert_eq!(remaining_actors(c), vec![p(1, 1), p(1, 0)]);
    }

    #[test]
    fn retreat_in_last_slot_still_costs_a_parting_blow() {
        let mut c = scenario(6);
        act(&mut c, false);
        act(&mut c, false);
        let t = act(&mut c, true);
        assert_eq!(t.actor, p(1, 0));
        assert_eq!(remaining_actors(c), vec![p(0, 0)]);
    }

    #[test]
    fn dead_enemies_strike_no_parting_blow() {
        let mut c = scenario(6);
        c.opponent.members[1].health = 0.0;
        act(&mut c, true);
        assert_eq!(remaining_actors(c), vec![p(1, 0)]);
    }

    /// Makes the actor of `turn` flee on its own and starts the parting blows.
    fn flee_member(c: &mut Conflict, turn: &Turn) {
        let party = turn.actor.party_id;
        let id = turn.actor.member_id;
        let p = if party == c.initiator.id {
            &mut c.initiator
        } else {
            &mut c.opponent
        };
        p.members[id].fled = true;
        c.start_parting(turn, party, vec![id]);
    }

    #[test]
    fn single_flight_resumes_the_round() {
        let mut c = scenario(6);
        let t = c.next_turn().unwrap();
        c.finish_turn(&t);
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(1, 1));
        flee_member(&mut c, &t);
        let t = c.next_turn().unwrap();
        assert_eq!((t.round, t.slot, t.actor.clone()), (1, 0, p(0, 0)));
        c.finish_turn(&t);
        assert!(c.parting.is_none());
        let t = c.next_turn().unwrap();
        assert_eq!((t.round, t.slot, t.actor.clone()), (1, 2, p(1, 0)));
        c.finish_turn(&t);
        let t = c.next_turn().unwrap();
        assert_eq!((t.round, t.slot, t.actor), (2, 0, p(0, 0)));
    }

    #[test]
    fn parting_ends_when_the_fleeing_fighters_die() {
        let mut c = scenario(6);
        c.initiator
            .members
            .push(member(1, 5, 20.0, Weapon::Fists(Fists { damage: 10.0 })));
        c.opponent.members.remove(1);
        // Order: hero 0 (6), hero 1 (5), villain 0 (3).
        for _ in 0..2 {
            let t = c.next_turn().unwrap();
            c.finish_turn(&t);
        }
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(1, 0));
        flee_member(&mut c, &t);
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(0, 0));
        c.opponent.members[0].health = 0.0; // the parting blow kills it
        c.finish_turn(&t);
        assert!(c.parting.is_none());
        assert_eq!(c.next_turn(), None);
        assert!(c.opponent.is_defeated());
    }

    #[test]
    fn last_fighter_fleeing_ends_after_parting() {
        let mut c = scenario(6);
        c.opponent.members.remove(1);
        let t = c.next_turn().unwrap();
        c.finish_turn(&t);
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(1, 0));
        flee_member(&mut c, &t);
        assert!(c.opponent.is_gone());
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(0, 0));
        c.finish_turn(&t);
        assert_eq!(c.next_turn(), None);
    }

    #[test]
    fn retreat_with_a_never_member_continues() {
        let mut c = scenario(6);
        c.opponent.members[0].morale = Morale::Never;
        let t = c.next_turn().unwrap();
        c.finish_turn(&t);
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(1, 1));
        let fled = c.opponent.retreat();
        assert_eq!(fled, vec![1]);
        c.start_parting(&t, 1, fled);
        let t = c.next_turn().unwrap();
        assert_eq!(t.actor, p(0, 0));
        c.finish_turn(&t);
        let t = c.next_turn().unwrap();
        assert_eq!((t.round, t.slot, t.actor), (1, 2, p(1, 0)));
    }

    #[test]
    fn fled_fighters_take_no_turns() {
        let mut c = scenario(6);
        c.opponent.members[1].fled = true;
        let mut actors = Vec::new();
        while let Some(t) = c.next_turn() {
            if t.round > 2 {
                break;
            }
            actors.push(t.actor.clone());
            c.finish_turn(&t);
        }
        assert_eq!(actors, vec![p(0, 0), p(1, 0), p(0, 0), p(1, 0)]);
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
