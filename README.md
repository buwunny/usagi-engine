# usagi engine

A fast, deterministic Riichi Mahjong engine in Rust, following Tenhou's
four-player ranked rules. It is the game engine behind usagi.club and the
training environment for bunny bot.

## Crates

| Crate | What it does |
| --- | --- |
| `usagi-core` | Tiles, hand parsing, shanten (table-driven, with a slow reference), waits, hand decomposition, yaku, fu and payments. No game state. |
| `usagi-engine` | The game: wall, dealing, turns, calls, riichi, kans, furiten, abortive draws, scoring and the end of the game. |
| `usagi-log` | Tenhou mjlog parser and a replayer that plays logged games through the engine and checks every action and score. |
| `usagi-mjai` | The Mjai protocol: a table that hosts four bots (in-process or child processes, mjai.app line protocol), and a rule-based baseline bot. |
| `usagi-obs` | One seat's observation as fixed-size feature planes (version 1), and a 155-way action numbering with legal-action masks. |

The Python bindings come in a later milestone.

## Replaying Tenhou logs

```sh
python3 tools/download_logs.py --year 2023 --month 1 --out data/logs/2023-01
cargo run --release -p usagi-log --bin usagi-replay -- data/logs/2023-01
```

The replayer rebuilds each hand's wall from the log, feeds every logged
decision to `step`, and checks that each action is legal, each draw is the
logged tile, each win has the logged han, fu and points, each hand's score
changes match, and (in hanchan logs) that each hand leads to the next one
and the last hand ends the game with the logged final scores. It prints
the first mismatch in each hand. `data/` is git-ignored.

69 sample logs (424 hands) from the [mjx](https://github.com/mjx-project/mjx)
project's tests run on every `cargo test`.

## Playing bots over Mjai

```sh
cargo build --release -p usagi-mjai
# Four baseline bots, ten games:
target/release/usagi-mjai play --games 10 baseline baseline baseline baseline
# Any bot that speaks the mjai.app protocol (such as Mortal) can take a seat:
target/release/usagi-mjai play --log game.jsonl "./path/to/bot" baseline baseline baseline
```

The table sends each bot a JSON array of the events since its last turn
(other seats' draws and hands hidden) and reads back one JSON action. The
engine checks every answer, so an illegal move stops the game with the
seat and the move. `usagi-mjai bot` runs the baseline bot on
stdin/stdout.

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
- replay of the sample Tenhou logs (`crates/usagi-log/tests/replay_samples.rs`),
- a leak test: shuffling every tile a seat can't see never changes its
  observation (`crates/usagi-obs/tests/observation.rs`),
- thousands of random and greedy games checking point conservation, tile
  conservation, hand sizes and determinism
  (`crates/usagi-engine/tests/invariants.rs`).

## License

MIT.
