# Turn-based Auto-battling Experiments

This is a toy project for experimenting with game strategies in a turn-based combat,
akin to fights in role-playing games like Might & Magic 3/4/5, Eye of the Beholder,
Bard's Tale, etc.

... with the twist of having ✨Artificial✨Intelligence✨ select your turns.

---

This is definitely not rocket science being done here. Expect more minimax than AlphaZero.

## Example outcome

The following is an example of how the default ambush may unfold. (Names come from the seed, so
they change between runs; the ones below are the fixed names the narration tests use.)

We see the hero party, consisting of _Brull_, as well as the enemy party, consisting of _Ziuon_ and _Molphige_.
_Ziuon_ deals only 5 damage with their stick, but _Molphige_ deals 20 with their fists, which will take
_Brull_ out in one hit. Fights are played in rounds, and within a round the fastest combatant acts first.
_Brull_ is the fastest (speed 6), so the only viable option is to take _Molphige_ out before he can swing.

That is indeed what happens. With _Molphige_ down, _Ziuon_ sees no way to win and flees at once,
taking _Brull_'s parting blow on the way out; waiting any longer would only cost more.
The lines marked _tactician_ explain each choice: what the acting side expects, and the best
alternatives it passed up, with their scores from the initiator's point of view
(`≤`/`≥` mark bounds that alpha-beta pruning left open; `≈` marks a score whose true value could lie on either side of the number).

```
⚔  Brull stands against Ziuon and Molphige.

══ Round 1 ══  initiative: Brull(6) → Molphige(5) → Ziuon(3)
Brull jabs Molphige — 10 damage. Molphige is down!
   ↳ tactician: expects they escape (2)
       alt: punch Ziuon    → defeat  (≤ -20)
       alt: glare at Ziuon → defeat  (≤ -20)
       +2 more
Ziuon sounds the retreat and flees!
   ↳ tactician: expects escape (2)
       alt: whack Brull    → defeat  (10)
       alt: glare at Brull → defeat  (≥ 14)
Parting blow: Brull jabs Ziuon — 10 damage.
   ↳ tactician: expects they escape (2)
       alt: glare at Ziuon → they escape  (2)

  Brull     ████████████████████ 20/20
  Ziuon     ↩ fled
  Molphige  ✝ fallen

🏁 Ziuon fled; Brull holds the field after 1 round.
```

A line with search statistics follows at the end. Run it with `task run`; the log plays back live
in a terminal, `task run -- --fast` prints it all at once and `task run -- --no-explain` leaves out
the tactician notes.

## Choosing a fight

The default fight is the ambush above. Other fights generate their parties from a seed: the same
seed builds the same stats and names on every run and machine. Runs without `--seed` pick one from
the clock and print it in the statistics line, so a good fight can be replayed.

| Command | Fight |
|---|---|
| `task run:scenarios` | List the presets |
| `task run:duel` | 1 hero vs 1 villain |
| `task run:skirmish` | 2 heroes vs 3 villains |
| `task run:brawl` | 3 heroes vs 4 villains |
| `task run:random HEROES=4 VILLAINS=2 SEED=7` | Any sizes from 1 to 4 per side; `SEED` is optional |

Every task takes extra flags after `--`, e.g. `task run:brawl -- --seed 42 --fast`. The flags:

- `--scenario NAME` picks a preset; `--heroes N` / `--villains N` generate custom sizes instead
  (a missing side defaults to 2 heroes or 3 villains).
- `--roster random` (default) rolls 10–25 health, fists (5–20 damage) or a stick (4–12) and speed
  1–9; `--roster archetypes` makes every member a Brawler, Fencer or Scout (heroes) or a Thug, Brute
  or Lurker (villains). The ambush keeps its fixed stats.
- `--seed N` replays a fight; `--depth N` sets how many actions ahead the tactician looks (1–40,
  default 10). Large fights get slow above about 16.

Every combatant searches anew on their turn, so a fight plays on until one side wins or escapes,
however far that is beyond the lookahead. After 50 rounds it is called undecided.

## Rules of ~~Engagement~~ the Game

- [x] Two factions are fighting each other in rounds. In every round each combatant acts once,
  in initiative order by speed (fastest first; ties: the initiating party first, then the lower member id).
- On their turn, each combatant picks one action. Such an action can be,
  - [x] Attacking a single opponent,
  - [ ] Attacking a group of opponents (area effects),
  - [ ] Applying an effect to a party member,
  - [ ] Skip the turn, i.e. do nothing.
- In addition, the faction as a whole can flee.
  - [x] Fleeing costs: every enemy still standing lands one parting blow, fastest first, then the fight ends.
- [ ] Party members can panic and either flee, be paralyzed or attack their own faction.
 
As for actions,

- [x] Some actions are instantaneous (attacking an enemy),
- [ ] Some actions take preparation (e.g. preparing a magic spell),
- [ ] Some actions have requirements (e.g. a potion must exist to be used)

## Things to do

- [x] Regular minimax ([`src/solver.rs`](src/solver.rs)).
- [x] Rounds with speed-ordered initiative, one combatant per turn ([`src/action_iterator.rs`](src/action_iterator.rs)).
- [x] Implement Alpha-Beta pruning ([`src/value.rs`](src/value.rs)).
- [x] Implement Iterative Deepening.
- [ ] Implement A*-like heuristics.
