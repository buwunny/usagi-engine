# usagi engine

A fast, deterministic Riichi Mahjong engine in Rust, following Tenhou's
four-player ranked rules. It is the game engine behind usagi.club and the
training environment for bunny bot.

## Crates

| Crate | What it does |
| --- | --- |
| `usagi-core` | Tiles, hand parsing, shanten (table-driven, with a slow reference), waits, hand decomposition, yaku, fu and payments. No game state. |
| `usagi-engine` | The game: wall, dealing, turns, calls, riichi, kans, furiten, abortive draws, scoring and the end of the game. |
| `usagi-obs` | One seat's observation as fixed-size feature planes (version 1), and a 155-way action numbering with legal-action masks. |
| `usagi-py` | Python bindings (`import usagi`): `Game`, and `VecEnv` for many games stepped in parallel with NumPy outputs. |

Log parsing and replay and Mjai come in later milestones.

## Python

```sh
pip install ./crates/usagi-py      # builds the Rust extension with maturin
python crates/usagi-py/examples/random_selfplay.py
```

```python
import usagi as mj

env = mj.VecEnv(1024, seed=0)           # 1024 games on Rust threads
seats, planes, scalars, scores, masks = env.observe()
actions = policy(planes, scalars, masks)  # one action index per game
done, final_scores = env.step(actions)  # finished games restart
```

`Game` steps one game a seat at a time and can be copied for search.
Observations and action indices follow `usagi-obs`; type stubs ship
with the package.

## Using the engine

```rust
use usagi_engine::{Action, GameState, Phase};

let mut game: GameState = GameState::new(42); // seeded, fully reproducible
let mut events = Vec::new();
while game.phase != Phase::GameEnd {
    if game.phase == Phase::RoundEnd {
        game.start_next_round(&mut events).unwrap();
        continue;
    }
    let seat = game.waiting_on().trailing_zeros() as u8;
    let action = game.legal_actions(seat)[0];
    game.step(seat, action, &mut events).unwrap();
}
println!("{:?}", game.scores);
```

- `GameState` is `Copy` and fits in 512 bytes (8 cache lines), so cloning
  it for search or rollouts is one memcpy. Compile-time asserts keep it there.
- `waiting_on()` is a bitmask of the seats that must act now. After a
  discard several seats may need to answer (ron, pon, chi or pass).
- `legal_actions(seat)` lists exactly what `step` accepts. Every action not
  in the list returns `Err(StepError::Illegal)`.
- `step` appends `Event`s (draws, discards, calls, new dora, wins, draws,
  payments) for logs and observations.
- `GameState::with_wall(seed, wall)` starts from a fixed wall, for tests and
  log replay.

## Rules

Tenhou four-player ranked (hanchan) rules, summarized in
[`docs/reference/tenhou-rules.md`](docs/reference/tenhou-rules.md), with the
yaku list and scoring tables next to it. Highlights: open tanyao, three red
fives, double ron (honba and riichi sticks to the first winner in turn
order), triple ron aborts, all furiten kinds, kuikae, kan dora timing,
chankan, nagashi mangan, pao for daisangen and daisuushi, tobi, and the
all-last and West-round extensions.

## Development

```sh
cargo test --workspace            # all tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The tests include:

- 38 scoring cases checked against an independent scorer
  (`crates/usagi-core/tests/scoring_cases.rs`),
- the table shanten compared with the reference on random hands,
- hand-built rule scenarios (`crates/usagi-engine/tests/scenarios.rs`),
- a leak test: shuffling every tile a seat can't see never changes its
  observation (`crates/usagi-obs/tests/observation.rs`),
- thousands of random and greedy games checking point conservation, tile
  conservation, hand sizes and determinism
  (`crates/usagi-engine/tests/invariants.rs`).

## License

MIT.
