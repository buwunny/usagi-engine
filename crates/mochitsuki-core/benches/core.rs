//! Shanten lookup and win scoring.
//!
//!     cargo bench -p mochitsuki-core

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mochitsuki_core::score::score;
use mochitsuki_core::shanten::shanten;
use mochitsuki_core::yaku::{Riichi, WinContext};
use mochitsuki_core::{Counts, Tile, tiles_of};

/// Random 13- or 14-tile hands from a fixed seed.
fn random_hands(n: usize, size: usize) -> Vec<Counts> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..n)
        .map(|_| {
            let mut wall: Vec<u8> = (0..136).map(|i| (i / 4) as u8).collect();
            for i in (1..wall.len()).rev() {
                wall.swap(i, (next() % (i as u64 + 1)) as usize);
            }
            let mut c = Counts::default();
            for &k in &wall[..size] {
                c.add(k);
            }
            c
        })
        .collect()
}

fn bench_shanten(c: &mut Criterion) {
    let hands13 = random_hands(1024, 13);
    let hands14 = random_hands(1024, 14);
    shanten(&hands13[0], 0); // build the tables outside the timing
    let mut i = 0;
    c.bench_function("shanten 13 tiles", |b| {
        b.iter(|| {
            i = (i + 1) % hands13.len();
            shanten(black_box(&hands13[i]), 0)
        })
    });
    c.bench_function("shanten 14 tiles", |b| {
        b.iter(|| {
            i = (i + 1) % hands14.len();
            shanten(black_box(&hands14[i]), 0)
        })
    });
}

fn bench_score(c: &mut Criterion) {
    // A riichi pinfu tsumo with ura, and a hand with several readings.
    let cases: Vec<(Vec<Tile>, WinContext)> = vec![
        (
            tiles_of("234m456p678s23455s"),
            WinContext {
                win_tile: tiles_of("5s")[0],
                tsumo: true,
                riichi: Riichi::Riichi,
                dora_indicators: tiles_of("1m"),
                ura_indicators: tiles_of("3s"),
                ..WinContext::default()
            },
        ),
        (
            tiles_of("111222333m789p55s"),
            WinContext {
                win_tile: tiles_of("3m")[0],
                ..WinContext::default()
            },
        ),
    ];
    for (i, (tiles, ctx)) in cases.iter().enumerate() {
        assert!(score(tiles, &[], ctx).is_some());
        c.bench_function(&format!("score win {i}"), |b| {
            b.iter(|| score(black_box(tiles), &[], black_box(ctx)))
        });
    }
}

criterion_group!(benches, bench_shanten, bench_score);
criterion_main!(benches);
