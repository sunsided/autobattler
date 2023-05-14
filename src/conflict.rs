use crate::party::Party;

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
    /// Sorts out the factions in terms of whose turn it currently is
    /// and who the current opponent would be.
    pub fn get_factions(&self) -> Factions {
        if is_even(self.turn) {
            Factions {
                current: &self.initiator,
                opponent: &self.opponent,
            }
        } else {
            Factions {
                current: &self.opponent,
                opponent: &self.initiator,
            }
        }
    }
}

/// A view on the factions.
pub struct Factions<'a> {
    /// The party whose turn it is.
    pub current: &'a Party,
    /// The other party.
    pub opponent: &'a Party,
}

/// Determines if a number is even.
const fn is_even(number: usize) -> bool {
    number & 1 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_even_works() {
        assert!(is_even(0));
        assert!(is_even(2));
        assert!(!is_even(1));
        assert!(!is_even(3));
    }
}
