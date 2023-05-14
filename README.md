# Turn-based Auto-battling Experiments

This is a toy project for experimenting with game strategies
in a turn-based combat game. More minimax than AlphaZero.

## Example outcome

A naive example may play out like this:

```
TL;DR: The initiating party wins with a score of 5.

On the attacking side:
- Agol, with 25 health

On the defending side:
- Örshashee, with 25 health

Turn 0:
  Agol whacks Örshashee with a stick, dealing 10 damage
   ⇒ Örshashee now has 15 health

Turn 1:
  Örshashee whacks Agol with a stick, dealing 10 damage
   ⇒ Agol now has 15 health

Turn 2:
  Agol whacks Örshashee with a stick, dealing 10 damage
   ⇒ Örshashee now has 5 health

Turn 3:
  Örshashee whacks Agol with a stick, dealing 10 damage
   ⇒ Agol now has 5 health

Turn 4:
  Agol whacks Örshashee with a stick, dealing 10 damage
   ⇒ Örshashee has given up on being alive
```

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
