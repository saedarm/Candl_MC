//! The board: squares, chutes/ladders, and the movement rule shared by
//! both the simulator and the Markov chain, so the two can never disagree.

use std::fmt;

/// What happens when a roll would carry you past the last square.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Overshoot {
    /// Don't move (the original programs' rule).
    Stay,
    /// Bounce back off the last square by the leftover amount.
    Bounce,
}

#[derive(Clone, Debug)]
pub struct Board {
    /// Index of the last square. Square 0 is the off-board start.
    pub last: usize,
    pub die: usize,
    pub overshoot: Overshoot,
    /// Jumps as entered, `(from, to)`, sorted by `from`.
    pub jumps: Vec<(usize, usize)>,
    /// For every square, where you actually end up after landing on it
    /// (chained jumps already followed).
    resolved: Vec<usize>,
}

#[derive(Debug)]
pub enum BoardError {
    TooSmall,
    BadDie,
    OutOfRange(usize, usize),
    StartsAtZero(usize),
    StartsOnLast(usize),
    SelfLoop(usize),
    DuplicateStart(usize),
    JumpCycle(Vec<usize>),
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::TooSmall => write!(f, "the board needs at least one square besides the start"),
            Self::BadDie => write!(f, "the die needs at least one side"),
            Self::OutOfRange(a, b) => write!(f, "jump {a}->{b} goes off the board"),
            Self::StartsAtZero(b) => write!(f, "jump 0->{b} starts on the off-board start square"),
            Self::StartsOnLast(b) => write!(f, "a jump can't start on the final square (->{b}); the game could never end"),
            Self::SelfLoop(a) => write!(f, "jump {a}->{a} goes nowhere"),
            Self::DuplicateStart(a) => write!(f, "square {a} is the start of more than one jump"),
            Self::JumpCycle(c) => {
                let path: Vec<String> = c.iter().map(|s| s.to_string()).collect();
                write!(f, "jumps form a loop with no way out: {}", path.join(" -> "))
            }
        }
    }
}

impl std::error::Error for BoardError {}

/// Milton Bradley's 1952 board: 9 ladders, 10 chutes, 100 squares.
pub const MILTON_BRADLEY: &[(usize, usize)] = &[
    (1, 38), (4, 14), (9, 31), (21, 42), (28, 84), (36, 44), (51, 67), (71, 91), (80, 100),
    (16, 6), (47, 26), (49, 11), (56, 53), (62, 19), (64, 60), (87, 24), (93, 73), (95, 75), (98, 78),
];

impl Board {
    #[allow(clippy::needless_range_loop)]
    pub fn new(
        last: usize,
        die: usize,
        overshoot: Overshoot,
        mut jumps: Vec<(usize, usize)>,
    ) -> Result<Self, BoardError> {
        if last == 0 {
            return Err(BoardError::TooSmall);
        }
        if die == 0 {
            return Err(BoardError::BadDie);
        }
        jumps.sort_unstable();
        let mut next: Vec<Option<usize>> = vec![None; last + 1];
        for &(a, b) in &jumps {
            if a > last || b > last {
                return Err(BoardError::OutOfRange(a, b));
            }
            if a == 0 {
                return Err(BoardError::StartsAtZero(b));
            }
            if a == last {
                return Err(BoardError::StartsOnLast(b));
            }
            if a == b {
                return Err(BoardError::SelfLoop(a));
            }
            if next[a].replace(b).is_some() {
                return Err(BoardError::DuplicateStart(a));
            }
        }

        // Follow chains (a ladder that lands on a chute head, etc.).
        // The original C++ stopped on the chute head; the Python silently
        // dropped that probability. Here both methods follow it through.
        let mut resolved = vec![0; last + 1];
        for sq in 0..=last {
            let mut path = vec![sq];
            let mut cur = sq;
            while let Some(n) = next[cur] {
                if let Some(i) = path.iter().position(|&p| p == n) {
                    let mut cycle = path[i..].to_vec();
                    cycle.push(n);
                    return Err(BoardError::JumpCycle(cycle));
                }
                path.push(n);
                cur = n;
            }
            resolved[sq] = cur;
        }

        Ok(Self { last, die, overshoot, jumps, resolved })
    }

    /// True if landing here sends you somewhere else (you never rest on it).
    pub fn is_jump_start(&self, sq: usize) -> bool {
        self.resolved[sq] != sq
    }

    /// Where a player on `from` ends the turn after rolling `roll`.
    pub fn step(&self, from: usize, roll: usize) -> usize {
        let target = from + roll;
        let landed = if target <= self.last {
            target
        } else {
            match self.overshoot {
                Overshoot::Stay => from,
                // Bounce off the end; clamp at 0 for dice bigger than the board.
                Overshoot::Bounce => self.last.saturating_sub(target - self.last),
            }
        };
        self.resolved[landed]
    }
}

/// Parse "a:b" or "a b" or "a->b".
pub fn parse_jump(s: &str) -> Result<(usize, usize), String> {
    let cleaned = s.replace("->", " ").replace([':', ','], " ");
    let nums: Vec<&str> = cleaned.split_whitespace().collect();
    match nums.as_slice() {
        [a, b] => Ok((
            a.parse().map_err(|_| format!("bad square '{a}'"))?,
            b.parse().map_err(|_| format!("bad square '{b}'"))?,
        )),
        _ => Err(format!("expected FROM:TO, got '{s}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_are_followed() {
        let b = Board::new(10, 6, Overshoot::Stay, vec![(2, 5), (5, 1)]).unwrap();
        assert_eq!(b.step(0, 2), 1);
    }

    #[test]
    fn jump_cycles_rejected() {
        let e = Board::new(10, 6, Overshoot::Stay, vec![(2, 5), (5, 2)]).unwrap_err();
        assert!(matches!(e, BoardError::JumpCycle(_)));
    }

    #[test]
    fn jump_on_last_square_rejected() {
        assert!(Board::new(10, 6, Overshoot::Stay, vec![(10, 3)]).is_err());
    }

    #[test]
    fn overshoot_rules() {
        let stay = Board::new(10, 6, Overshoot::Stay, vec![]).unwrap();
        let bounce = Board::new(10, 6, Overshoot::Bounce, vec![]).unwrap();
        assert_eq!(stay.step(8, 4), 8);
        assert_eq!(bounce.step(8, 4), 8); // 8 -> 10 -> back 2 = 8
        assert_eq!(bounce.step(9, 4), 7);
    }

    #[test]
    fn parse_formats() {
        assert_eq!(parse_jump("1:38").unwrap(), (1, 38));
        assert_eq!(parse_jump("98->78").unwrap(), (98, 78));
        assert_eq!(parse_jump(" 16 6 ").unwrap(), (16, 6));
    }
}
