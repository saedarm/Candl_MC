//! Monte Carlo: actually play the game a lot of times, in parallel.

use crate::board::Board;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use rayon::prelude::*;

pub struct Simulated {
    pub games: u64,
    pub mean_turns: f64,
    pub std_dev: f64,
    pub shortest: u64,
    pub longest: u64,
    /// Average landings per game on each square (index = square).
    pub visits: Vec<f64>,
}

struct Tally {
    turns: u64,
    turns_sq: u128,
    shortest: u64,
    longest: u64,
    visits: Vec<u64>,
}

impl Tally {
    fn new(len: usize) -> Self {
        Self { turns: 0, turns_sq: 0, shortest: u64::MAX, longest: 0, visits: vec![0; len] }
    }
    fn merge(mut self, o: Self) -> Self {
        self.turns += o.turns;
        self.turns_sq += o.turns_sq;
        self.shortest = self.shortest.min(o.shortest);
        self.longest = self.longest.max(o.longest);
        for (a, b) in self.visits.iter_mut().zip(o.visits) {
            *a += b;
        }
        self
    }
}

const CHUNK: u64 = 10_000;

/// Callers must check the board is winnable first (markov::solve does),
/// otherwise a game can loop forever.
pub fn run(board: &Board, games: u64, seed: u64) -> Simulated {
    let len = board.last + 1;
    let chunks = games.div_ceil(CHUNK);

    let tally = (0..chunks)
        .into_par_iter()
        .map(|c| {
            // One RNG per chunk, seeded from (seed, chunk), so results are
            // reproducible no matter how many threads run.
            let mut rng = StdRng::seed_from_u64(seed ^ c.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let n = CHUNK.min(games - c * CHUNK);
            let mut t = Tally::new(len);
            for _ in 0..n {
                let mut pos = 0;
                let mut turns = 0u64;
                while pos != board.last {
                    pos = board.step(pos, rng.random_range(1..=board.die));
                    t.visits[pos] += 1;
                    turns += 1;
                }
                t.turns += turns;
                t.turns_sq += (turns as u128) * (turns as u128);
                t.shortest = t.shortest.min(turns);
                t.longest = t.longest.max(turns);
            }
            t
        })
        .reduce(|| Tally::new(len), Tally::merge);

    let g = games as f64;
    let mean = tally.turns as f64 / g;
    let var = tally.turns_sq as f64 / g - mean * mean;
    Simulated {
        games,
        mean_turns: mean,
        std_dev: var.max(0.0).sqrt(),
        shortest: tally.shortest,
        longest: tally.longest,
        visits: tally.visits.iter().map(|&v| v as f64 / g).collect(),
    }
}
