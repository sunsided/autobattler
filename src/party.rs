use crate::party_member::PartyMember;
use std::fmt::{Display, Formatter};

/// A party, or faction in a conflict.
#[derive(Debug, Clone)]
pub struct Party {
    /// The ID of the party. Must be unique in the conflict.
    pub id: usize,
    /// All members of the party.
    pub members: Vec<PartyMember>,
    /// Indicates if the party has retreated from the encounter.
    pub retreated: bool,
}

/// A participant.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Participant {
    /// The ID of the targeted party.
    pub party_id: usize,
    /// The ID of the targeted member in the party.
    pub member_id: usize,
}

impl Party {
    /// Makes every member unable to act in the encounter.
    pub fn retreat(&mut self) {
        self.retreated = true;
        for member in self.members.iter_mut() {
            member.can_act = false;
        }
    }

    /// Returns `true` if the party is defeated.
    ///
    /// ## Defeat
    /// A party is considered defeated if either all party members
    /// are deceased or have fled the conflict.
    pub fn is_defeated(&self) -> bool {
        self.members.iter().all(PartyMember::is_dead)
    }

    /// Returns `true` if the party has retreated from the encounter.
    pub fn has_retreated(&self) -> bool {
        self.retreated
    }

    /// Returns `true` if at least one party can still act.
    pub fn can_act(&self) -> bool {
        self.members.iter().any(PartyMember::can_act)
    }

    /// Replaces the member identified by the party member's ID with the
    /// new value provided here.
    pub fn replace_member(&mut self, member: PartyMember) {
        for m in self.members.iter_mut() {
            if m.id == member.id {
                *m = member;
                break;
            }
        }
    }

    /// Returns the size of the party.
    pub fn len(&self) -> usize {
        self.members.len()
    }
}

impl Display for Participant {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.party_id, self.member_id)
    }
}
