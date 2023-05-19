# Turn-based Auto-battling Experiments

This is a toy project for experimenting with game strategies in a turn-based combat,
akin to fights in role-playing games like Might & Magic 3/4/5, Eye of the Beholder,
Bard's Tale, etc.

... with the twist of having ✨Artificial✨Intelligence✨ select your turns.

---

This is definitely not rocket science being done here. Expect more minimax than AlphaZero.

## Things to do

- [x] Regular minimax.
- [x] Minimax with multiple turns per party.
- [x] Implement Alpha-Beta pruning.
- [ ] Implement Iterative Deepening.

## Example outcome

The following is an example of how it may unfold.

We see the hero party, consisting of _Harubs_, as well as the enemy party, consisting of _Denah_ and _Peoul_. 
_Denah_ deals only 5 damage with their stick, but _Peoul_ deals 20 with their fists, which will take
_Harubs_ out in one hit. Regardless of _Peoul_ being the third one to move, the only viable option is
for _Harubs_ to attack _Peoul_ first to ensure they can never make a move to begin with.

That is indeed what happens:

```
TL;DR: The initiating party wins with a score of 10.

On the attacking side:
- Harubs, with 20 health and their fists (10 damage)

On the defending side:
- Denah, with 15 health and a stick (5 damage)
- Peoul, with 10 health and their fists (20 damage)

Turn 1:
  Harubs whacks Peoul with their fists, dealing 10 damage
   ⇒ Peoul has given up on being alive

Turn 2:
  Denah whacks Harubs with a stick, dealing 5 damage
   ⇒ Harubs now has 15 health

Turn 3:
  Harubs whacks Denah with their fists, dealing 10 damage
   ⇒ Denah now has 5 health

Turn 4:
  Denah whacks Harubs with a stick, dealing 5 damage
   ⇒ Harubs now has 10 health

Turn 5:
  Harubs whacks Denah with their fists, dealing 10 damage
   ⇒ Denah has given up on being alive
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
