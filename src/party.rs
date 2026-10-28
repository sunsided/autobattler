use crate::party_member::{Morale, PartyMember};
use std::fmt::{Display, Formatter};

/// A party, or faction in a conflict.
#[derive(Debug, Clone)]
pub struct Party {
    /// The ID of the party. Must be unique in the conflict.
    pub id: usize,
    /// All members of the party.
    pub members: Vec<PartyMember>,
    /// Indicates whether the party is allowed to retreat.
    pub can_retreat: bool,
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
    /// Makes every living member without `Never` morale flee the encounter.
    ///
    /// ## Returns
    /// The IDs of the members that fled, in member order.
    pub fn retreat(&mut self) -> Vec<usize> {
        debug_assert!(self.can_retreat);
        let mut fled = Vec::new();
        for member in self.members.iter_mut() {
            if !member.is_dead() && !member.fled && member.morale != Morale::Never {
                member.fled = true;
                fled.push(member.id);
            }
        }
        fled
    }

    /// Returns `true` if no member is left on the field.
    pub fn is_gone(&self) -> bool {
        !self.members.iter().any(PartyMember::on_field)
    }

    /// Returns `true` if the party is defeated.
    ///
    /// ## Defeat
    /// A party is considered defeated if either all party members
    /// are deceased or have fled the conflict.
    pub fn is_defeated(&self) -> bool {
        self.members.iter().all(PartyMember::is_dead)
    }

    /// Returns `true` if the party is able to retreat.
    pub fn can_retreat(&self) -> bool {
        self.can_retreat
    }

    /// Returns `true` if the party has retreated from the encounter.
    pub fn has_retreated(&self) -> bool {
        self.is_gone() && self.members.iter().any(|m| m.fled)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::{Stick, Weapon};

    fn member(id: usize, morale: Morale) -> PartyMember {
        PartyMember {
            id,
            health: 10.0,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 1.0 }),
            can_act: true,
            speed: 1,
            morale,
            fled: false,
        }
    }

    fn party(members: Vec<PartyMember>) -> Party {
        Party {
            id: 0,
            members,
            can_retreat: true,
        }
    }

    #[test]
    fn retreat_leaves_never_members_behind() {
        let mut p = party(vec![
            member(0, Morale::Tactical),
            member(1, Morale::Never),
            member(2, Morale::Courage(0.3)),
        ]);
        assert_eq!(p.retreat(), vec![0, 2]);
        assert!(p.members[1].on_field());
        assert!(!p.is_gone());
        assert!(!p.has_retreated());
    }

    #[test]
    fn party_gone_when_all_dead_or_fled() {
        let mut p = party(vec![
            member(0, Morale::Tactical),
            member(1, Morale::Tactical),
        ]);
        p.members[0].health = 0.0;
        p.members[1].fled = true;
        assert!(p.is_gone());
        assert!(p.has_retreated());
        assert!(!p.is_defeated());
    }
}
