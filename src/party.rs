use crate::party_member::PartyMember;

/// A party, or faction in a conflict.
#[derive(Debug, Clone)]
pub struct Party {
    /// The ID of the party. Must be unique in the conflict.
    pub id: usize,
    /// All members of the party.
    pub members: Vec<PartyMember>,
}

impl Party {
    /// Returns `true` if the party is defeated.
    ///
    /// ## Defeat
    /// A party is considered defeated if either all party members
    /// are deceased or have fled the conflict.
    pub fn is_defeated(&self) -> bool {
        self.members.iter().all(PartyMember::is_dead)
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
}