use crate::party::{Participant, Party};
use crate::party_member::PartyMember;

/// A conflict, specifically the state of conflict at a given turn.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// The turn number
    pub turn: usize,
    /// The party initiating the conflict.
    pub initiator: Party,
    /// The other involved party.
    pub opponent: Party,
}

impl Conflict {
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
