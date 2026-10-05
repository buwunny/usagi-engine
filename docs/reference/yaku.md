# Yaku reference (Tenhou, four players)

Han values as **closed / open**. "—" means the yaku is not allowed in an
open hand. A closed kan (*ankan*) does not open the hand.

A hand needs **at least one yaku** to win. Dora, red fives and ura dora
add han but are not yaku.

Conditions below are written the way you'll check them in code. "Set"
means sequence, triplet or kan (called melds included unless stated);
"the pair" is the head.

## 1 han

| Yaku | Han | Condition |
|---|---|---|
| Riichi | 1 / — | Declared riichi. (Double riichi replaces it, see below.) |
| Ippatsu | 1 / — | Won within one go-around after riichi, before your next discard, with no call by anyone in between (a closed kan also breaks it). |
| Menzen tsumo | 1 / — | Won by tsumo with a closed hand. |
| Pinfu | 1 / — | Closed; all four sets are sequences; the pair is not a dragon, your seat wind or the round wind; the wait is two-sided (*ryanmen*). If the winning tile can be read as ryanmen in any reading, that reading can be pinfu. |
| Iipeikou | 1 / — | Closed; two identical sequences (same suit, same numbers). |
| Tanyao | 1 / 1 | No terminals or honors anywhere (melds included). Open allowed on Tenhou (*kuitan*). |
| Yakuhai: Haku | 1 / 1 | Triplet or kan of White dragon. |
| Yakuhai: Hatsu | 1 / 1 | Triplet or kan of Green dragon. |
| Yakuhai: Chun | 1 / 1 | Triplet or kan of Red dragon. |
| Yakuhai: seat wind | 1 / 1 | Triplet or kan of your seat wind. |
| Yakuhai: round wind | 1 / 1 | Triplet or kan of the round wind. Stacks with seat wind (East triplet for the dealer in the East round = 2 han). |
| Rinshan kaihou | 1 / 1 | Won by tsumo on the replacement tile after a kan. |
| Chankan | 1 / 1 | Won by ron on a tile another player added to a pon (kakan). |
| Haitei raoyue | 1 / 1 | Won by tsumo on the last drawable tile. Not with rinshan (the replacement after a kan on the last tile is rinshan, not haitei). |
| Houtei raoyui | 1 / 1 | Won by ron on the last discard. |

## 2 han

| Yaku | Han | Condition |
|---|---|---|
| Double riichi | 2 / — | Riichi declared on your first discard with no calls by anyone before it. Replaces riichi (not added to it). |
| Chiitoitsu | 2 / — | Seven pairs of seven different kinds. Always 25 fu. |
| Sanshoku doujun | 2 / 1 | The same sequence in all three suits (e.g. 345m 345p 345s). |
| Ittsu (ikkitsuukan) | 2 / 1 | 123, 456 and 789 of the same suit. |
| Chanta | 2 / 1 | Every set and the pair contain a terminal or honor, and there is at least one sequence and at least one honor. (No honors: that's junchan. No sequences: that's honroutou/toitoi.) |
| Toitoi | 2 / 2 | All four sets are triplets or kans. |
| Sanankou | 2 / 2 | Three *concealed* triplets/kans. A triplet completed by ron is not concealed; closed kans are. The hand itself may be open. |
| Sanshoku doukou | 2 / 2 | Triplets/kans of the same number in all three suits. |
| Sankantsu | 2 / 2 | Three kans of any type. |
| Honroutou | 2 / 2 | Every tile is a terminal or honor. Always comes with toitoi or chiitoitsu. Excludes chanta. |
| Shousangen | 2 / 2 | Two dragon triplets/kans and a dragon pair. (The two dragon triplets also score their yakuhai, so it's effectively 4 han.) |

## 3 han and up

| Yaku | Han | Condition |
|---|---|---|
| Honitsu | 3 / 2 | Only one suit plus honors (and at least one honor; otherwise it's chinitsu). |
| Junchan | 3 / 2 | Every set and the pair contain a terminal, no honors at all, at least one sequence. Replaces chanta. |
| Ryanpeikou | 3 / — | Closed; two pairs of identical sequences, e.g. 223344m 556677p (four copies of one sequence also counts). Replaces iipeikou. A ryanpeikou hand is also seven pairs; score whichever is higher (ryanpeikou always is). |
| Chinitsu | 6 / 5 | Every tile from one suit, no honors. Replaces honitsu. |

## Yakuman (each counts as 13 han = one limit)

On Tenhou, every yakuman is a **single** yakuman, including the 13-sided
kokushi, single-wait suuankou, nine-sided chuuren and daisuushi. Different
yakuman in one hand **do** stack (daisangen + tsuuiisou = double).
Normal yaku and dora are ignored when any yakuman is present.

| Yakuman | Condition |
|---|---|
| Kokushi musou | One of each of the 13 terminals/honors plus one duplicate. Closed only. |
| Suuankou | Four concealed triplets/kans. Closed. Won by tsumo, or by ron on a single (tanki) wait. A ron that completes a triplet makes it open, which leaves only sanankou + toitoi. |
| Daisangen | Triplets/kans of all three dragons. |
| Shousuushi | Three wind triplets/kans and a wind pair. |
| Daisuushi | Four wind triplets/kans. |
| Tsuuiisou | Only honor tiles (with sets or seven pairs). |
| Ryuuiisou | Only 2s 3s 4s 6s 8s and Green dragon. Green dragon not required. |
| Chinroutou | Only terminals (1 and 9), no honors. |
| Chuuren poutou | Closed, one suit: 1112345678999 plus any one extra tile of that suit. |
| Suukantsu | Four kans. |
| Tenhou | Dealer wins on the initial deal (first draw), before any call. |
| Chiihou | Non-dealer wins by tsumo on their first draw, before any call. |

Not on Tenhou: renhou, open riichi, daisharin, and the other local yakuman.

## Kazoe yakuman

A normal hand reaching 13+ han (yaku + dora) scores as a yakuman (8000
base). On Tenhou this is capped at a single yakuman.

## Exclusions, summarized

These never both count; keep only the first:
- Double riichi over riichi
- Ryanpeikou over iipeikou
- Junchan over chanta
- Chinitsu over honitsu
- Honroutou over chanta
- Any yakuman over everything that isn't a yakuman

## Situations that are easy to forget

- **Yakuhai and pinfu**: a yakuhai *pair* doesn't make yakuhai (only a
  triplet does), but it does kill pinfu.
- **Open pinfu shape**: an open hand with all sequences and a two-sided
  wait has no pinfu, but still gets the 30-fu treatment (see
  [scoring.md](scoring.md)).
- **Multiple readings**: `111222333m` is three triplets (sanankou) or
  three identical sequences (iipeikou, maybe pinfu). Score both, keep
  the better.
- **Winning tile position**: with `123m 345m` and a winning 3m, the 3m
  could have completed either sequence: a penchan (12 + 3) or a ryanmen
  (45 + 3). Both readings exist; pinfu and fu differ.
