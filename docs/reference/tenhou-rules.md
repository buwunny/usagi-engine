# Tenhou rules for the engine (M5)

The flow rules the engine has to enforce, beyond hand scoring. Items
marked **(verify)** are ones to confirm against real logs during M6
before trusting them; when the replayer disagrees with this page, the
logs win, and you fix this page.

## Setup

- 4 players, 25,000 points each, East round then South round (*hanchan*).
- 136 tiles with three red fives (one each of 5m, 5p, 5s).
- Open tanyao (*kuitan*) allowed; winning on a yaku you only gain from
  the winning tile (*atozuke*) allowed.

## Turn and calls

- Dealer starts with 14 tiles (in usagi engine's flow: deal 13 each, then
  the dealer draws).
- Call priority on a discard: **ron > pon = daiminkan > chi**. If
  several players ron, see "multiple ron".
- Chi only from the player directly before you in turn order.
- After chi or pon you discard immediately, without drawing.
- **No swap-calling (*kuikae*)**: after a chi or pon you may not discard
  the tile you just called, nor (for chi) the tile at the other end of
  the same sequence. E.g. chi 4m with 56m: you may not discard 4m or 7m
  that turn. After a pon of 5p you may not discard 5p.
- Nothing can be called from the last discard of the hand (*houtei*)
  except ron.
- No kan when there is no tile left to draw (haitei), and no fifth kan
  in a hand.

## Kans

- **Ankan**: on your turn, four of a kind from your hand.
- **Kakan**: on your turn, add the fourth tile to your own pon. Other
  players may ron on that tile (*chankan*); if nobody does, the kan
  stands.
- **Daiminkan**: on a discard, holding the other three.
- Every kan draws a replacement tile from the dead wall (*rinshan*) and
  reveals a new dora indicator. Timing on Tenhou: an **ankan** reveals
  its new dora immediately; an **open kan** (daiminkan, kakan) reveals it
  after the replacement-draw discard **(verify)**.
- Kan in riichi: only an ankan, only with the tile just drawn, and only
  if it doesn't change the hand's waits **(verify the exact Tenhou
  condition: some rulesets also require the hand's reading to stay the
  same)**.

## Riichi

- Requirements: closed hand (ankan allowed), tenpai after the discard,
  at least 1000 points, and at least 4 tiles left in the live wall.
- The 1000 stick goes on the table when the riichi discard is **not**
  ronned. If that discard is ronned, the riichi is cancelled and no
  stick is paid.
- After riichi, every drawn tile not used to win or ankan must be
  discarded.
- Double riichi: riichi on your first discard with no calls by anyone
  before it.
- Ippatsu: ends at the riichi player's next discard, or when anyone
  calls (including any kan).

## Furiten (no ron; tsumo still allowed)

1. **Discard furiten**: one of your current waits is in your own discard
   pond (including tiles that were called away from it).
2. **Temporary furiten**: you passed on a ron this go-around; lasts until
   your next discard.
3. **Riichi furiten**: you're in riichi and passed on a ron; lasts for
   the rest of the hand.

Furiten depends on the *waits* of the hand, not on whether the specific
tile has yaku: if any wait is in your pond, you can't ron on any of them.

## Multiple ron

- **Double ron** is allowed: both win, each paid separately by the
  discarder. Riichi sticks go to the winner first in turn order after
  the discarder; honba handling **(verify)**.
- **Triple ron** aborts the hand (*sanchahou*) as an abortive draw.

## Abortive draws (*tochuu ryuukyoku*)

All repeat the hand with the same dealer and honba + 1. Riichi sticks
stay on the table.

| Name | When |
|---|---|
| Kyuushu kyuuhai | On your first draw, uninterrupted by any call, you hold 9+ *different* terminal/honor kinds. Optional: you may choose to play on. |
| Suufon renda | All four players discard the same wind on their first discard, with no calls. |
| Suucha riichi | All four players declare riichi (aborts when the fourth riichi discard passes without ron). |
| Suukaikan | A fourth kan is declared and the four kans are not all by one player (aborts after the kan's discard passes without ron) **(verify timing)**. |
| Sanchahou | Three players ron the same discard. |

## Exhaustive draw (*ryuukyoku*)

- When the live wall is empty and the last discard passes.
- Tenpai players receive the noten payments (see
  [scoring.md](scoring.md)). A hand whose only wait is a tile you hold all
  four of counts as **noten** on Tenhou **(verify)**.
- Nagashi mangan replaces the tenpai payments when it applies.
- Dealer tenpai: dealer repeats. Dealer noten: dealer passes. Honba + 1
  either way.

## Dealer rotation and honba

| Result | Dealer | Honba |
|---|---|---|
| Dealer wins | repeats | + 1 |
| Non-dealer wins | passes to next | reset to 0 |
| Exhaustive draw, dealer tenpai | repeats | + 1 |
| Exhaustive draw, dealer noten | passes | + 1 |
| Abortive draw | repeats | + 1 |

## Game end

- **Tobi**: the game ends immediately when anyone's score goes **below**
  zero (exactly 0 keeps playing).
- After **South 4** (*oorasu*): the game ends if anyone has 30,000 or
  more. Otherwise it continues into the West round.
- In **South 4**, if the dealer would repeat but is in first place with
  30,000+, the game ends there (Tenhou applies this automatically,
  *agari-yame* / *tenpai-yame*) **(verify the exact conditions)**.
- In the **West round**, the game ends at the end of any hand where
  someone has 30,000+. West 4 is the final hand regardless.
- Leftover riichi sticks go to the first-place player.
- Ties in final score are broken by seat order: the player closer to the
  starting dealer (East 1's dealer) ranks higher.

## Liability (*pao*)

If a player completes daisangen or daisuushi by calling the last needed
set from another player's discard, that player is liable: on tsumo they
pay the full amount; on ron by someone else, they split it with the
discarder **(verify which yakuman Tenhou applies this to)**.
