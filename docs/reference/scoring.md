# Scoring reference (Tenhou)

## 1. Fu

Start at **20** (*futei*), add everything that applies, then **round up
to the next 10**. Exceptions first:

| Case | Fu |
|---|---|
| Chiitoitsu | exactly 25, never rounded |
| Pinfu + tsumo | exactly 20 (the tsumo +2 is not added) |
| Pinfu + ron | 30 (20 + 10 closed ron) |
| Open hand that would come to 20 (all sequences, two-sided wait, non-yakuhai pair, ron) | raised to 30 |
| Yakuman | fu don't matter |

Additions:

| Source | Fu |
|---|---|
| Closed hand won by ron (*menzen kafu*) | +10 |
| Tsumo (except pinfu tsumo) | +2 |
| Wait: kanchan, penchan or tanki | +2 |
| Wait: ryanmen or shanpon | 0 |
| Pair of a dragon | +2 |
| Pair of your seat wind | +2 |
| Pair of the round wind | +2 (a double-wind pair is +4 on Tenhou) |

Sets (sequences are always 0):

| Set | Simples (2-8) | Terminals / honors |
|---|---|---|
| Open triplet (pon, or a triplet completed by **ron**) | 2 | 4 |
| Concealed triplet | 4 | 8 |
| Open kan (daiminkan or kakan) | 8 | 16 |
| Closed kan (ankan) | 16 | 32 |

**When a wait can be read two ways**, each reading is scored separately
and the best payment wins. E.g. `123m 345m` won on 3m: penchan (+2 fu,
no pinfu) or ryanmen (pinfu possible). Your `readings()` produces both; `score()` keeps the better.

### Worked example

`777m 333s 444z 55z` + pon `222p`, ron on 4z (shanpon with 55z), seat
South, round East. (Test case `toitoi_ron_shanpon_triplet_is_open`.)

| Part | Fu |
|---|---|
| Base | 20 |
| Open hand, so no closed-ron bonus | 0 |
| pon 222p: open simple triplet | 2 |
| 777m concealed simple triplet | 4 |
| 333s concealed simple triplet | 4 |
| 444z completed by ron: **open** honor triplet | 4 |
| Pair 55z (White dragon) | 2 |
| Shanpon wait | 0 |
| Total | 36 → **40** |

## 2. Base points

```
base = fu × 2^(2 + han)        for han 1..4 and capped at 2000
```

| Han | Name | Base |
|---|---|---|
| 1-4 (base ≥ 2000) or 5 | mangan | 2000 |
| 6-7 | haneman | 3000 |
| 8-10 | baiman | 4000 |
| 11-12 | sanbaiman | 6000 |
| 13+ | (kazoe) yakuman | 8000 |
| each yakuman | yakuman | 8000 × count |

**No kiriage mangan on Tenhou.** 4 han 30 fu = 1920 base = 7700 from a
non-dealer, not 8000. (3 han 60 fu likewise = 1920.)

## 3. Payments

Every individual payment is **rounded up to 100** separately.

| Win | Who pays |
|---|---|
| Non-dealer ron | discarder pays base × 4 |
| Dealer ron | discarder pays base × 6 |
| Non-dealer tsumo | dealer pays base × 2, each non-dealer pays base × 1 |
| Dealer tsumo | each player pays base × 2 |

That rounding is why a non-dealer 3 han 20 fu tsumo totals 2700 (700 +
700 + 1300), not 2560.

### Common values (non-dealer)

| | 20 fu | 25 fu | 30 fu | 40 fu | 50 fu | 60 fu | 70 fu |
|---|---|---|---|---|---|---|---|
| 1 han ron | — | — | 1000 | 1300 | 1600 | 2000 | 2300 |
| 1 han tsumo | — | — | 300/500 | 400/700 | 400/800 | 500/1000 | 600/1200 |
| 2 han ron | — | 1600 | 2000 | 2600 | 3200 | 3900 | 4500 |
| 2 han tsumo | 400/700 | 400/800 | 500/1000 | 700/1300 | 800/1600 | 1000/2000 | 1200/2300 |
| 3 han ron | — | 3200 | 3900 | 5200 | 6400 | 7700 | 8000 |
| 3 han tsumo | 700/1300 | 800/1600 | 1000/2000 | 1300/2600 | 1600/3200 | 2000/3900 | 2000/4000 |
| 4 han ron | — | 6400 | 7700 | 8000 | 8000 | 8000 | 8000 |
| 4 han tsumo | 1300/2600 | 1600/3200 | 2000/3900 | 2000/4000 | ... | | |

Tsumo values are "non-dealer pays / dealer pays". Limits: mangan 8000
(2000/4000), haneman 12000, baiman 16000, sanbaiman 24000, yakuman 32000.

### Common values (dealer)

| | 30 fu | 40 fu |
|---|---|---|
| 1 han ron | 1500 | 2000 |
| 2 han ron | 2900 | 3900 |
| 3 han ron | 5800 | 7700 |
| 4 han ron | 11600 | 12000 |
| 1 han tsumo | 500 all | 700 all |

Dealer limits: mangan 12000 (4000 all), haneman 18000, baiman 24000,
sanbaiman 36000, yakuman 48000.

Your `score::payment` tests check several of these. Add more from these
tables if you like; they're easy to type and catch rounding bugs.

## 4. Honba and riichi sticks

- **Honba** (repeat counters): +300 per honba for each win, paid by the
  discarder on ron, or 100 per honba by each payer on tsumo.
- **Riichi sticks**: each declaration puts 1000 on the table. The next
  winner takes all of them. If the game ends with sticks still on the
  table, on Tenhou they go to the first-place player.
- **Double ron**: each winner gets their own payment from the discarder.
  The riichi sticks go to the winner who comes first in turn order after
  the discarder. How Tenhou splits the honba in a double ron is one of
  the details to confirm from real logs in M6 (search the replay for a
  double ron with honba > 0); record what you find in a test.

## 5. Exhaustive draw (ryuukyoku) payments

If the wall runs out, tenpai players get 3000 in total from noten
players:

| Tenpai players | Each tenpai gets | Each noten pays |
|---|---|---|
| 0 or 4 | nothing | nothing |
| 1 | 3000 | 1000 |
| 2 | 1500 | 1500 |
| 3 | 1000 | 3000 |

**Nagashi mangan**: at an exhaustive draw, a player whose every discard
was a terminal or honor and none of whose discards were called is paid
as a mangan **tsumo**. It replaces the tenpai payments. Whether honba and
riichi sticks move with it on Tenhou is another detail to pin down from
logs in M6.
