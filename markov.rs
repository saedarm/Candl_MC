//! Exact answers from absorbing Markov chain theory.
//!
//! Q = transitions between non-final squares, N = (I - Q)^-1 is the
//! fundamental matrix. N[i][j] = expected visits to j starting from i,
//! t = N·1 = expected turns to finish. Instead of inverting, we LU-factor
//! (I - Q) once and solve for just the vectors we need.

use crate::board::Board;
use std::collections::VecDeque;

pub struct Exact {
    /// Resting squares in matrix order (excludes jump starts, unreachable
    /// squares, and the final square).
    pub states: Vec<usize>,
    pub expected_turns: f64,
    pub std_dev: f64,
    /// Expected landings on each square of the board (index = square),
    /// not counting the starting position. Final square is always 1.
    pub visits: Vec<f64>,
    /// P(game takes exactly k turns), index k.
    pub length_pmf: Vec<f64>,
    lu_t: Lu, // LU of (I - Q)^T, kept for the full matrix export
}

#[derive(Debug)]
pub struct Unwinnable(pub usize);

impl std::fmt::Display for Unwinnable {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "a player who reaches square {} can never finish", self.0)
    }
}
impl std::error::Error for Unwinnable {}

pub fn solve(board: &Board) -> Result<Exact, Unwinnable> {
    let last = board.last;
    let die = board.die;
    let p = 1.0 / die as f64;

    // Which squares can a player actually stand on, starting from 0?
    let mut reach = vec![false; last + 1];
    let mut queue = VecDeque::from([0]);
    reach[0] = true;
    while let Some(s) = queue.pop_front() {
        if s == last {
            continue;
        }
        for r in 1..=die {
            let n = board.step(s, r);
            if !reach[n] {
                reach[n] = true;
                queue.push_back(n);
            }
        }
    }

    // Every reachable square must be able to reach the end. This replaces
    // the old "too many turns" failsafe with an exact check.
    let mut finishes = vec![false; last + 1];
    finishes[last] = true;
    loop {
        let mut changed = false;
        for s in 0..last {
            if reach[s] && !finishes[s] && (1..=die).any(|r| finishes[board.step(s, r)]) {
                finishes[s] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    if let Some(stuck) = (0..last).rev().find(|&s| reach[s] && !finishes[s]) {
        return Err(Unwinnable(stuck));
    }

    let states: Vec<usize> = (0..last).filter(|&s| reach[s]).collect();
    let n = states.len();
    let mut index = vec![usize::MAX; last + 1];
    for (i, &s) in states.iter().enumerate() {
        index[s] = i;
    }

    // A = I - Q, and R = one-turn probability of finishing from each state.
    let mut a = vec![0.0; n * n];
    let mut finish_now = vec![0.0; n];
    for (i, &s) in states.iter().enumerate() {
        a[i * n + i] += 1.0;
        for r in 1..=die {
            let to = board.step(s, r);
            if to == last {
                finish_now[i] += p;
            } else {
                a[i * n + index[to]] -= p;
            }
        }
    }

    let lu = Lu::new(a.clone(), n);
    let t = lu.solve(vec![1.0; n]);
    let u = lu.solve(t.clone());
    let start = index[0];
    let var = 2.0 * u[start] - t[start] - t[start] * t[start];

    // Row 0 of N = solve N^T e_0, i.e. (I - Q)^T x = e_0.
    let mut at = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            at[j * n + i] = a[i * n + j];
        }
    }
    let lu_t = Lu::new(at, n);
    let mut e0 = vec![0.0; n];
    e0[start] = 1.0;
    let row0 = lu_t.solve(e0);

    let mut visits = vec![0.0; last + 1];
    for (i, &s) in states.iter().enumerate() {
        visits[s] = row0[i];
    }
    visits[0] -= 1.0; // N counts the starting position; the simulator doesn't
    visits[last] = 1.0;

    // Game-length distribution: push probability mass forward turn by turn.
    let mut dist = vec![0.0; n];
    dist[start] = 1.0;
    let mut pmf = vec![0.0];
    let mut remaining = 1.0;
    while remaining > 1e-12 && pmf.len() < 1_000_000 {
        let mut next = vec![0.0; n];
        let mut done = 0.0;
        for (i, &s) in states.iter().enumerate() {
            if dist[i] == 0.0 {
                continue;
            }
            done += dist[i] * finish_now[i];
            for r in 1..=die {
                let to = board.step(s, r);
                if to != last {
                    next[index[to]] += dist[i] * p;
                }
            }
        }
        pmf.push(done);
        remaining -= done;
        dist = next;
    }

    Ok(Exact {
        states,
        expected_turns: t[start],
        std_dev: var.max(0.0).sqrt(),
        visits,
        length_pmf: pmf,
        lu_t,
    })
}

impl Exact {
    /// Smallest k with P(T <= k) >= q.
    pub fn quantile(&self, q: f64) -> usize {
        let mut acc = 0.0;
        for (k, &pk) in self.length_pmf.iter().enumerate() {
            acc += pk;
            if acc >= q {
                return k;
            }
        }
        self.length_pmf.len() - 1
    }

    pub fn shortest(&self) -> usize {
        self.length_pmf.iter().position(|&x| x > 0.0).unwrap_or(0)
    }

    /// The full fundamental matrix N, row-major, over `self.states`.
    pub fn fundamental_matrix(&self) -> Vec<Vec<f64>> {
        let n = self.states.len();
        // Column j of N^T = row j of N.
        (0..n)
            .map(|j| {
                let mut e = vec![0.0; n];
                e[j] = 1.0;
                self.lu_t.solve(e)
            })
            .collect()
    }
}

/// Dense LU factorization with partial pivoting.
struct Lu {
    n: usize,
    lu: Vec<f64>,
    perm: Vec<usize>,
}

impl Lu {
    fn new(mut m: Vec<f64>, n: usize) -> Self {
        let mut perm: Vec<usize> = (0..n).collect();
        for k in 0..n {
            let pivot = (k..n)
                .max_by(|&x, &y| m[x * n + k].abs().total_cmp(&m[y * n + k].abs()))
                .unwrap();
            if pivot != k {
                for c in 0..n {
                    m.swap(k * n + c, pivot * n + c);
                }
                perm.swap(k, pivot);
            }
            let d = m[k * n + k];
            for r in k + 1..n {
                let f = m[r * n + k] / d;
                if f == 0.0 {
                    continue;
                }
                m[r * n + k] = f;
                for c in k + 1..n {
                    m[r * n + c] -= f * m[k * n + c];
                }
            }
        }
        Self { n, lu: m, perm }
    }

    fn solve(&self, b: Vec<f64>) -> Vec<f64> {
        let n = self.n;
        let mut x: Vec<f64> = self.perm.iter().map(|&i| b[i]).collect();
        for r in 0..n {
            for c in 0..r {
                x[r] -= self.lu[r * n + c] * x[c];
            }
        }
        for r in (0..n).rev() {
            for c in r + 1..n {
                x[r] -= self.lu[r * n + c] * x[c];
            }
            x[r] /= self.lu[r * n + r];
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Overshoot, MILTON_BRADLEY};

    #[test]
    fn standard_board_matches_original_python() {
        let b = Board::new(100, 6, Overshoot::Stay, MILTON_BRADLEY.to_vec()).unwrap();
        let e = solve(&b).unwrap();
        assert!((e.expected_turns - 39.22512231).abs() < 1e-6, "{}", e.expected_turns);
        let total: f64 = e.length_pmf.iter().sum();
        assert!((total - 1.0).abs() < 1e-9);
    }

    #[test]
    fn empty_board_one_sided_die() {
        // Every turn moves exactly one square: 10 turns, zero variance.
        let b = Board::new(10, 1, Overshoot::Stay, vec![]).unwrap();
        let e = solve(&b).unwrap();
        assert!((e.expected_turns - 10.0).abs() < 1e-9);
        assert!(e.std_dev < 1e-6);
    }

    #[test]
    fn detects_trap() {
        // d2 from square 5: 6 and 7 both chute back to 5. Stuck forever.
        let b = Board::new(10, 2, Overshoot::Stay, vec![(6, 5), (7, 5)]).unwrap();
        assert!(solve(&b).is_err());
    }
}
