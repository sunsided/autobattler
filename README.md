# Turn-based Auto-battling Experiments

This is a toy project for experimenting with game strategies in a turn-based combat,
akin to fights in role-playing games like Might & Magic 3/4/5, Eye of the Beholder,
Bard's Tale, etc.

... with the twist of having ✨Artificial✨Intelligence✨ select your turns.

---

This is definitely not rocket science being done here. Expect more minimax than AlphaZero.

## Things to do

- [x] Regular minimax ([`src/solver.rs`](src/solver.rs)).
- [x] Multiple turns per party ([`src/action_iterator.rs`](src/action_iterator.rs)).
- [x] Implement Alpha-Beta pruning ([`src/value.rs`](src/value.rs)).
- [ ] Implement Iterative Deepening.

## Example outcome

The following is an example of how it may unfold.

We see the hero party, consisting of _Brull_, as well as the enemy party, consisting of _Ziuon_ and _Molphige_. 
_Ziuon_ deals only 5 damage with their stick, but _Molphige_ deals 20 with their fists, which will take
_Brull_ out in one hit. Regardless of _Molphige_ being the third one to move, the only viable option is
for _Brull_ to attack _Molphige_ first to ensure they can never make a move to begin with.

That is indeed what happens:

```
Performed 921 evaluations with 221 cuts at depth 8 in 309.766µs. The encounter has 5 turns.
TL;DR: The initiating party wins with a score of 10.

On the attacking side:
- Brull, with 20 health and their fists (10 damage)

On the defending side:
- Ziuon, with 15 health and a stick (5 damage)
- Molphige, with 10 health and their fists (20 damage)

Turn 1 (discovered at step 1):
  Brull whacks Molphige with their fists, dealing 10 damage
   ⇒ Molphige has given up on being alive

Turn 2 (discovered at step 2):
  Ziuon whacks Brull with a stick, dealing 5 damage
   ⇒ Brull now has 15 health

Turn 3 (discovered at step 3):
  Brull whacks Ziuon with their fists, dealing 10 damage
   ⇒ Ziuon now has 5 health

Turn 4 (discovered at step 4):
  Ziuon whacks Brull with a stick, dealing 5 damage
   ⇒ Brull now has 10 health

Turn 5 (discovered at step 5):
  Brull whacks Ziuon with their fists, dealing 10 damage
   ⇒ Ziuon has given up on being alive
```

If retreats are allowed for the enemy party, the game has a different outcome: The enemy
now decides to flee at their first chance. The hero party still gets an extra move, but it is not
enough to result in a defeat of the enemy.

```
Performed 1063 evaluations with 247 cuts at depth 8 in 360.599µs. The encounter has 3 turns.
TL;DR: The initiating party let the opponent flee with a score of 2.

On the attacking side:
- Brull, with 20 health and their fists (10 damage)

On the defending side:
- Ziuon, with 15 health and a stick (5 damage)
- Molphige, with 10 health and their fists (20 damage)

Turn 1 (discovered at step 1):
  Brull whacks Molphige with their fists, dealing 10 damage
   ⇒ Molphige has given up on being alive

Turn 2 (discovered at step 2):
  the party flees

Turn 3 (discovered at step 3):
  Brull whacks Ziuon with their fists, dealing 10 damage
   ⇒ Ziuon now has 5 health
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
