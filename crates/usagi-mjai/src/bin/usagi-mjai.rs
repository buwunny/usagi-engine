//! Mjai command line.
//!
//!     usagi-mjai bot
//!         Runs the baseline bot over stdin/stdout (mjai.app protocol: one
//!         JSON array of events per input line, one JSON action per output
//!         line).
//!
//!     usagi-mjai play [--seed N] [--games K] [--log FILE] BOT BOT BOT BOT
//!         Hosts games for four bots, seat 0 first. Each BOT is `baseline`
//!         (in-process) or a shell command that speaks the protocol, for
//!         example a Mortal mjai bot. Game i uses seed N + i. With --log,
//!         every game's full Mjai log is written to FILE, one event per
//!         line.

use std::io::{BufRead, Write};
use std::process::ExitCode;

use usagi_mjai::{Baseline, Bot, Event, ProcessBot, play_game};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("bot") => run_bot(),
        Some("play") => play(&args[1..]),
        _ => Err(
            "usage: usagi-mjai bot | play [--seed N] [--games K] [--log FILE] BOT BOT BOT BOT"
                .into(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run_bot() -> Result<(), String> {
    let mut bot = Baseline::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let events: Vec<Event> =
            serde_json::from_str(&line).map_err(|e| format!("bad input {line:?}: {e}"))?;
        let answer = bot.react(&events)?;
        let out = serde_json::to_string(&answer).map_err(|e| e.to_string())?;
        writeln!(stdout, "{out}").map_err(|e| e.to_string())?;
        stdout.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn play(args: &[String]) -> Result<(), String> {
    let (mut seed, mut games, mut log_path) = (0u64, 1u64, None);
    let mut specs = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--seed" => seed = value()?.parse().map_err(|_| "bad --seed")?,
            "--games" => games = value()?.parse().map_err(|_| "bad --games")?,
            "--log" => log_path = Some(value()?.clone()),
            _ => specs.push(a.clone()),
        }
    }
    if specs.len() != 4 {
        return Err(format!("need exactly 4 bots, got {}", specs.len()));
    }
    let mut log = match &log_path {
        Some(p) => Some(std::io::BufWriter::new(
            std::fs::File::create(p).map_err(|e| format!("{p}: {e}"))?,
        )),
        None => None,
    };
    let mut rank_sum = [0u64; 4];
    for i in 0..games {
        let mut bots: Vec<Box<dyn Bot>> = Vec::new();
        for spec in &specs {
            bots.push(if spec == "baseline" {
                Box::new(Baseline::new())
            } else {
                Box::new(ProcessBot::spawn(spec).map_err(|e| format!("{spec}: {e}"))?)
            });
        }
        let [b0, b1, b2, b3] = &mut bots[..] else {
            unreachable!()
        };
        let record = play_game(
            seed + i,
            [b0.as_mut(), b1.as_mut(), b2.as_mut(), b3.as_mut()],
        )
        .map_err(|e| format!("game {i} (seed {}): {e}", seed + i))?;
        if let Some(w) = log.as_mut() {
            for e in &record.log {
                writeln!(w, "{}", serde_json::to_string(e).unwrap()).map_err(|e| e.to_string())?;
            }
        }
        let mut order = [0usize, 1, 2, 3];
        order.sort_by_key(|&s| (-record.scores[s], s));
        for (rank, &s) in order.iter().enumerate() {
            rank_sum[s] += rank as u64 + 1;
        }
        println!("game {i} (seed {}): scores {:?}", seed + i, record.scores);
    }
    let avg: Vec<String> = rank_sum
        .iter()
        .map(|&r| format!("{:.2}", r as f64 / games as f64))
        .collect();
    println!("average rank by seat: {}", avg.join(" "));
    Ok(())
}
