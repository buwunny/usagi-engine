//! Python bindings: `import usagi`.
//!
//! - `Game`: one game, stepped one decision at a time.
//! - `VecEnv`: many games stepped in parallel on Rust threads, with the
//!   GIL released, returning batched NumPy arrays.
//!
//! Actions are indices into the fixed action space of `usagi-obs`
//! (`NUM_ACTIONS` of them); observations use its encoding (`OBS_VERSION`).

use numpy::{PyArray1, PyArray2, PyArray3, PyArrayMethods, PyReadonlyArray1};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use rayon::prelude::*;
use usagi_core::tile::NUM_KINDS;
use usagi_engine::{GameState, Phase};
use usagi_obs::encode::{plane, scalar};
use usagi_obs::{NUM_ACTIONS, Observation, action_at, encode_into, legal_mask};

type State = GameState;

const NUM_PLANES: usize = plane::COUNT;
const NUM_SCALARS: usize = scalar::COUNT;

/// The seat that acts next: the lowest seat being waited on.
fn acting_seat(g: &State) -> Option<u8> {
    let w = g.waiting_on();
    (w != 0).then(|| w.trailing_zeros() as u8)
}

fn phase_name(g: &State) -> &'static str {
    match g.phase {
        Phase::Turn { .. } => "turn",
        Phase::AfterCall { .. } => "after_call",
        Phase::CallWindow { .. } => "call",
        Phase::ChankanWindow { .. } => "chankan",
        Phase::RoundEnd => "round_end",
        Phase::GameEnd => "game_end",
    }
}

/// Applies action `index` for `seat`.
fn apply(g: &mut State, seat: u8, index: usize) -> Result<(), String> {
    let action = action_at(g, seat, index)
        .ok_or_else(|| format!("action {index} is not legal for seat {seat}"))?;
    g.step(seat, action, &mut Vec::new())
        .map_err(|e| format!("{action:?}: {e:?}"))
}

/// One game of four-player Tenhou-rules Mahjong.
#[pyclass(module = "usagi")]
#[derive(Clone)]
struct Game {
    g: State,
}

#[pymethods]
impl Game {
    #[new]
    #[pyo3(signature = (seed = 0))]
    fn new(seed: u64) -> Self {
        Game {
            g: State::new(seed),
        }
    }

    /// Starts a new game from `seed`.
    fn reset(&mut self, seed: u64) {
        self.g = State::new(seed);
    }

    /// A copy of this game (for search or rollouts).
    fn copy(&self) -> Game {
        self.clone()
    }

    /// One of "turn", "after_call", "call", "chankan", "round_end",
    /// "game_end".
    #[getter]
    fn phase(&self) -> &'static str {
        phase_name(&self.g)
    }

    /// Seats that must decide now (several during a call window).
    #[getter]
    fn waiting(&self) -> Vec<u8> {
        (0..4)
            .filter(|s| self.g.waiting_on() & 1 << s != 0)
            .collect()
    }

    /// The next seat to act, or None between hands and after the game.
    #[getter]
    fn current_seat(&self) -> Option<u8> {
        acting_seat(&self.g)
    }

    #[getter]
    fn scores(&self) -> [i32; 4] {
        self.g.scores
    }

    #[getter]
    fn done(&self) -> bool {
        self.g.phase == Phase::GameEnd
    }

    /// Final placement of each seat, 0 = first.
    fn ranks(&self) -> [u8; 4] {
        self.g.ranks()
    }

    /// Legal action indices for `seat`.
    fn legal_actions(&self, seat: u8) -> Vec<usize> {
        let mask = legal_mask(&self.g, seat);
        (0..NUM_ACTIONS).filter(|&i| mask[i] != 0).collect()
    }

    /// 0/1 mask over all `NUM_ACTIONS` indices.
    fn legal_mask<'py>(&self, py: Python<'py>, seat: u8) -> Bound<'py, PyArray1<u8>> {
        PyArray1::from_slice(py, &legal_mask(&self.g, seat))
    }

    /// A readable name for action `index` if it is legal for `seat`.
    fn describe(&self, seat: u8, index: usize) -> Option<String> {
        action_at(&self.g, seat, index).map(|a| format!("{a:?}"))
    }

    /// `seat` takes action `index`. Raises ValueError if it isn't legal.
    fn step(&mut self, seat: u8, index: usize) -> PyResult<()> {
        apply(&mut self.g, seat, index).map_err(PyValueError::new_err)
    }

    /// Deals the next hand; only between hands.
    fn next_round(&mut self) -> PyResult<()> {
        self.g
            .start_next_round(&mut Vec::new())
            .map_err(|e| PyValueError::new_err(format!("{e:?}")))
    }

    /// `seat`'s observation: (planes uint8 [NUM_PLANES, 34], scalars uint8
    /// [NUM_SCALARS], scores int32 [4], relative to `seat`).
    #[allow(clippy::type_complexity)]
    fn observation<'py>(
        &self,
        py: Python<'py>,
        seat: u8,
    ) -> (
        Bound<'py, PyArray2<u8>>,
        Bound<'py, PyArray1<u8>>,
        Bound<'py, PyArray1<i32>>,
    ) {
        let mut obs = Observation::default();
        encode_into(&self.g, seat, &mut obs);
        let planes = PyArray1::from_slice(py, obs.plane_bytes())
            .reshape([NUM_PLANES, NUM_KINDS])
            .expect("plane size");
        (
            planes,
            PyArray1::from_slice(py, &obs.scalars),
            PyArray1::from_slice(py, &obs.scores),
        )
    }

    fn __repr__(&self) -> String {
        format!(
            "Game(round={}, honba={}, phase={}, scores={:?})",
            self.g.round.index,
            self.g.round.honba,
            phase_name(&self.g),
            self.g.scores
        )
    }
}

/// `num_envs` games stepped together. Every game is always waiting for
/// one seat's decision: hands are dealt automatically, and a finished game
/// is replaced by a new one with the next seed.
#[pyclass(module = "usagi")]
struct VecEnv {
    games: Vec<State>,
    next_seed: u64,
}

/// Moves `g` past hand ends to the next decision; returns final scores if
/// the game ended.
fn advance(g: &mut State) -> Option<[i32; 4]> {
    loop {
        match g.phase {
            Phase::RoundEnd => {
                g.start_next_round(&mut Vec::new())
                    .expect("RoundEnd can always deal");
            }
            Phase::GameEnd => return Some(g.scores),
            _ => return None,
        }
    }
}

#[pymethods]
impl VecEnv {
    #[new]
    #[pyo3(signature = (num_envs, seed = 0))]
    fn new(num_envs: usize, seed: u64) -> PyResult<Self> {
        if num_envs == 0 {
            return Err(PyValueError::new_err("num_envs must be positive"));
        }
        let games = (0..num_envs as u64).map(|i| State::new(seed + i)).collect();
        Ok(VecEnv {
            games,
            next_seed: seed + num_envs as u64,
        })
    }

    #[getter]
    fn num_envs(&self) -> usize {
        self.games.len()
    }

    /// The deciding seat and its observation and legal mask, for every
    /// game: (seats uint8 [N], planes uint8 [N, NUM_PLANES, 34], scalars
    /// uint8 [N, NUM_SCALARS], scores int32 [N, 4], masks uint8 [N,
    /// NUM_ACTIONS]).
    #[allow(clippy::type_complexity)]
    fn observe<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<(
        Bound<'py, PyArray1<u8>>,
        Bound<'py, PyArray3<u8>>,
        Bound<'py, PyArray2<u8>>,
        Bound<'py, PyArray2<i32>>,
        Bound<'py, PyArray2<u8>>,
    )> {
        let n = self.games.len();
        let seats = PyArray1::<u8>::zeros(py, [n], false);
        let planes = PyArray3::<u8>::zeros(py, [n, NUM_PLANES, NUM_KINDS], false);
        let scalars = PyArray2::<u8>::zeros(py, [n, NUM_SCALARS], false);
        let scores = PyArray2::<i32>::zeros(py, [n, 4], false);
        let masks = PyArray2::<u8>::zeros(py, [n, NUM_ACTIONS], false);
        {
            // SAFETY: the arrays were just created here, are contiguous,
            // and nothing else can reach them until they are returned.
            let (s, p, c, sc, m) = unsafe {
                (
                    seats.as_slice_mut()?,
                    planes.as_slice_mut()?,
                    scalars.as_slice_mut()?,
                    scores.as_slice_mut()?,
                    masks.as_slice_mut()?,
                )
            };
            let games = &self.games;
            py.allow_threads(|| {
                s.par_iter_mut()
                    .zip(p.par_chunks_mut(NUM_PLANES * NUM_KINDS))
                    .zip(c.par_chunks_mut(NUM_SCALARS))
                    .zip(sc.par_chunks_mut(4))
                    .zip(m.par_chunks_mut(NUM_ACTIONS))
                    .zip(games.par_iter())
                    .for_each(|(((((seat, p), c), sc), m), g)| {
                        let Some(st) = acting_seat(g) else { return };
                        *seat = st;
                        let mut obs = Observation::default();
                        encode_into(g, st, &mut obs);
                        p.copy_from_slice(obs.plane_bytes());
                        c.copy_from_slice(&obs.scalars);
                        sc.copy_from_slice(&obs.scores);
                        m.copy_from_slice(&legal_mask(g, st));
                    });
            });
        }
        Ok((seats, planes, scalars, scores, masks))
    }

    /// Applies one action index per game for its deciding seat. Returns
    /// (done bool [N], final_scores int32 [N, 4]); a finished game's row
    /// holds its final scores and the game is restarted with a new seed.
    #[allow(clippy::type_complexity)]
    fn step<'py>(
        &mut self,
        py: Python<'py>,
        actions: PyReadonlyArray1<'py, i64>,
    ) -> PyResult<(Bound<'py, PyArray1<bool>>, Bound<'py, PyArray2<i32>>)> {
        let actions = actions.as_slice()?;
        let n = self.games.len();
        if actions.len() != n {
            return Err(PyValueError::new_err(format!(
                "expected {n} actions, got {}",
                actions.len()
            )));
        }
        let results: Vec<Result<Option<[i32; 4]>, String>> = py.allow_threads(|| {
            self.games
                .par_iter_mut()
                .zip(actions.par_iter())
                .enumerate()
                .map(|(i, (g, &a))| {
                    let seat =
                        acting_seat(g).ok_or_else(|| format!("env {i}: no decision pending"))?;
                    let index = usize::try_from(a)
                        .map_err(|_| format!("env {i}: action {a} out of range"))?;
                    apply(g, seat, index).map_err(|e| format!("env {i}: {e}"))?;
                    Ok(advance(g))
                })
                .collect()
        });
        let done = PyArray1::<bool>::zeros(py, [n], false);
        let finals = PyArray2::<i32>::zeros(py, [n, 4], false);
        {
            // SAFETY: freshly created, contiguous, not shared.
            let (d, f) = unsafe { (done.as_slice_mut()?, finals.as_slice_mut()?) };
            for (i, r) in results.into_iter().enumerate() {
                if let Some(scores) = r.map_err(PyValueError::new_err)? {
                    d[i] = true;
                    f[i * 4..i * 4 + 4].copy_from_slice(&scores);
                    self.games[i] = State::new(self.next_seed);
                    self.next_seed += 1;
                }
            }
        }
        Ok((done, finals))
    }
}

#[pymodule]
fn usagi(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Game>()?;
    m.add_class::<VecEnv>()?;
    m.add("NUM_ACTIONS", NUM_ACTIONS)?;
    m.add("NUM_PLANES", NUM_PLANES)?;
    m.add("NUM_SCALARS", NUM_SCALARS)?;
    m.add("OBS_VERSION", usagi_obs::VERSION)?;
    Ok(())
}
