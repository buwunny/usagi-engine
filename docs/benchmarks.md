# Benchmarks

Run with `cargo bench -p usagi-core -p usagi-engine` and
`cargo run --release -p usagi-engine --example throughput -- [SECONDS] [THREADS]`.

Numbers below are from a 4-core Xeon at 2.10GHz (cloud VM), release build.

| Benchmark | Result |
| --- | --- |
| Shanten, 13 or 14 random tiles | ~95 ns |
| Score a winning hand | 0.6 to 1.2 µs |
| Copy `GameState` (512 bytes) | 14 ns |
| One random-policy game | 1.85 ms (~1066 decisions) |
| Random games, 4 threads | ~2100 games/s |
| Observation encode | ~3.8 µs |

## Where the time goes

A callgrind profile of random games puts about 80% of engine time in
shanten, mostly called from `legal_actions` (riichi and win checks), 10% in
`can_ron` and 9% in `mark_missed_wins`. Checking 14-tile shanten before
trying each riichi discard cut a game from 2.3 ms to 1.85 ms.

## Against the Phase 0 targets

The plan's target of 10,000 random games/s assumes a larger machine; this
VM reaches about 530 games/s per core. With a neural policy, a self-play
decision costs far more than the ~1.7 µs the engine spends per decision, so
the engine stays well under 10% of a step. Further work (a cached shanten
per seat, a wait table for ron checks) waits until a real training profile
shows the engine above that 10%.
