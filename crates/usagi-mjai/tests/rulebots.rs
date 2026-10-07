//! The rule bots play legal games at every level and can explain every
//! move they make.

use usagi_mjai::explain::Reason;
use usagi_mjai::{Bot, Event, Explain, Level, RuleBot, play_game, suggest};

/// Wraps a bot and checks its explanation after every decision.
struct Checked {
    bot: RuleBot,
    explained: usize,
    /// Every event this seat was sent.
    history: Vec<Event>,
}

impl Bot for Checked {
    fn react(&mut self, events: &[Event]) -> Result<Event, String> {
        self.history.extend_from_slice(events);
        let answer = self.bot.react(events)?;
        // The `dahai` after our own `reach` repeats the turn's decision.
        let after_reach = matches!(events, [Event::Reach { .. }]);
        if answer != Event::None && !after_reach {
            let e = self.bot.explain().ok_or("no explanation")?;
            if e.chosen != answer {
                return Err(format!("explained {:?} but played {answer:?}", e.chosen));
            }
            let json = serde_json::to_string(&e).map_err(|x| x.to_string())?;
            let back: usagi_mjai::Explanation =
                serde_json::from_str(&json).map_err(|x| x.to_string())?;
            if back != e {
                return Err("explanation doesn't round-trip through JSON".into());
            }
            if e.summary().is_empty() {
                return Err("empty summary".into());
            }
            // A hint from the same seat's events says the same thing.
            if self.bot.level() == Level::Hard {
                let start = self
                    .history
                    .iter()
                    .rposition(|e| matches!(e, Event::StartKyoku { .. }))
                    .unwrap();
                let mut since = vec![self.history[0].clone()];
                since.extend_from_slice(&self.history[start..]);
                let hint = suggest(Level::Hard, &since).ok_or("no hint")?;
                if hint != e {
                    return Err(format!(
                        "hint {:?} differs from play {:?}",
                        hint.chosen, e.chosen
                    ));
                }
            }
            self.explained += 1;
        }
        Ok(answer)
    }
}

#[test]
fn every_level_plays_legal_games_and_explains_them() {
    let levels = Level::ALL;
    let mut explained = 0;
    for seed in 0..24u64 {
        let mut bots: Vec<Checked> = (0..4)
            .map(|s| Checked {
                bot: RuleBot::new(
                    levels[(s + seed as usize) % levels.len()],
                    seed * 4 + s as u64,
                ),
                explained: 0,
                history: Vec::new(),
            })
            .collect();
        let [b0, b1, b2, b3] = &mut bots[..] else {
            unreachable!()
        };
        let record = play_game(seed, [b0 as &mut dyn Bot, b1, b2, b3]).unwrap_or_else(|e| {
            panic!(
                "seed {seed}: {e}\nlast events: {:?}",
                &e.log[e.log.len().saturating_sub(12)..]
            )
        });
        assert!(matches!(record.log.last(), Some(Event::EndGame)));
        explained += bots.iter().map(|b| b.explained).sum::<usize>();
    }
    assert!(explained > 5000, "{explained}");
}

#[test]
fn hard_bots_call_and_still_finish() {
    let mut calls = 0;
    for seed in 0..20u64 {
        let mut bots: Vec<RuleBot> = (0..4).map(|s| RuleBot::new(Level::Hard, s)).collect();
        let [b0, b1, b2, b3] = &mut bots[..] else {
            unreachable!()
        };
        let record = play_game(seed, [b0 as &mut dyn Bot, b1, b2, b3]).unwrap();
        calls += record
            .log
            .iter()
            .filter(|e| matches!(e, Event::Chi { .. } | Event::Pon { .. }))
            .count();
    }
    assert!(calls > 50, "{calls}");
}

#[test]
fn normal_beats_easy() {
    let games = 120u64;
    let mut rank_sum = 0u64;
    for g in 0..games {
        let hero = (g % 4) as usize;
        let mut bots: Vec<RuleBot> = (0..4)
            .map(|s| {
                RuleBot::new(
                    if s == hero {
                        Level::Normal
                    } else {
                        Level::Easy
                    },
                    g * 4 + s as u64,
                )
            })
            .collect();
        let [b0, b1, b2, b3] = &mut bots[..] else {
            unreachable!()
        };
        let r = play_game(500 + g, [b0 as &mut dyn Bot, b1, b2, b3]).unwrap();
        let me = r.scores[hero];
        rank_sum += 1
            + (0..4)
                .filter(|&s| r.scores[s] > me || r.scores[s] == me && s < hero)
                .count() as u64;
    }
    let avg = rank_sum as f64 / games as f64;
    assert!(avg < 1.9, "normal's average rank against easy: {avg}");
}

/// Counts riichi and quiet (damaten) choices from the explanations.
struct Riichis {
    bot: RuleBot,
    declared: usize,
    quiet: usize,
}

impl Bot for Riichis {
    fn react(&mut self, events: &[Event]) -> Result<Event, String> {
        let answer = self.bot.react(events)?;
        if matches!(events, [Event::Reach { .. }]) {
            return Ok(answer);
        }
        let Some(e) = self.bot.explain() else {
            return Ok(answer);
        };
        let quiet = e.reasons.iter().any(|r| {
            matches!(
                r,
                Reason::Dama { .. }
                    | Reason::DeadWait
                    | Reason::DamaUnderAttack
                    | Reason::DamaToImprove { .. }
            )
        });
        if quiet {
            if !matches!(answer, Event::Dahai { .. }) {
                return Err(format!("stayed quiet but played {answer:?}"));
            }
            self.quiet += 1;
        }
        self.declared += matches!(answer, Event::Reach { .. }) as usize;
        Ok(answer)
    }
}

#[test]
fn hard_bots_sometimes_stay_quiet_instead_of_riichi() {
    let (mut declared, mut quiet) = (0, 0);
    for seed in 0..40u64 {
        let mut bots: Vec<Riichis> = (0..4)
            .map(|s| Riichis {
                bot: RuleBot::new(Level::Hard, seed * 4 + s),
                declared: 0,
                quiet: 0,
            })
            .collect();
        let [b0, b1, b2, b3] = &mut bots[..] else {
            unreachable!()
        };
        play_game(900 + seed, [b0 as &mut dyn Bot, b1, b2, b3]).unwrap();
        declared += bots.iter().map(|b| b.declared).sum::<usize>();
        quiet += bots.iter().map(|b| b.quiet).sum::<usize>();
    }
    assert!(declared > 100, "only {declared} riichi");
    assert!(quiet > 30, "only {quiet} quiet turns");
}

#[test]
fn levels_have_names() {
    for l in Level::ALL {
        assert_eq!(Level::from_name(l.name()), Some(l));
    }
    assert_eq!(Level::from_name("impossible"), None);
}
