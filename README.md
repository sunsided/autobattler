# Turn-based Auto-battling Experiments

This is a toy project for experimenting with game strategies
in a turn-based combat game. More minimax than AlphaZero.

## Rules of ~~Engagement~~ the Game

- Two factions are fighting each other and take turns
  in making moves.
- Within each faction, a party of one or more participants
  is allowed to make a move. Such an action can be,
  - Attacking a single opponent,
  - Attacking a group of opponents (area effects),
  - Applying an effect to a party member,
  - Skip the turn, i.e. do nothing.
- In addition, the faction as a whole can flee.
 
As for actions,

- Some actions are instantaneous (attacking an enemy),
- Some actions take preparation (e.g. preparing a magic spell),
- Some actions have requirements (e.g. a potion must exist to be used)

### Future additions

- Party members can panic and either flee, be paralyzed or attack their own faction.
