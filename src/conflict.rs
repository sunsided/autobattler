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
