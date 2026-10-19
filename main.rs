mod board;
mod markov;
mod sim;

use board::{Board, Overshoot, MILTON_BRADLEY};
use clap::Parser;
use std::error::Error;
use std::fs;
use std::io::Write;

/// Chutes and Ladders, two ways: play it a million times, then get the
/// exact answer from the Markov chain, and compare.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Playable squares (the off-board start square 0 is added for you)
    #[arg(short, long, default_value_t = 100)]
    squares: usize,

    /// Sides on the die
    #[arg(short, long, default_value_t = 6)]
    die: usize,

    /// A chute or ladder as FROM:TO (repeatable). With no --jump and no
    /// --board, the 1952 Milton Bradley board is used.
    #[arg(short, long = "jump", value_parser = board::parse_jump)]
    jumps: Vec<(usize, usize)>,

    /// File of jumps, one "FROM TO" per line, '#' for comments
    #[arg(short, long)]
    board: Option<String>,

    /// Play with no chutes or ladders at all
    #[arg(long, conflicts_with_all = ["jumps", "board"])]
    empty: bool,

    /// What a roll past the last square does
    #[arg(long, value_enum, default_value_t = Overshoot::Stay)]
    overshoot: Overshoot,

    /// Games to simulate (0 = exact answer only)
    #[arg(short, long, default_value_t = 1_000_000)]
    games: u64,

    /// RNG seed, for reproducible runs
    #[arg(long)]
    seed: Option<u64>,

    /// Print expected landings on every square
    #[arg(long)]
    per_square: bool,

    /// Write the full fundamental matrix N to this CSV file
    #[arg(long)]
    matrix: Option<String>,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Box<dyn Error>> {
    let (jumps, label) = if args.empty {
        (vec![], "no chutes or ladders".to_string())
    } else if let Some(path) = &args.board {
        let mut j = read_board_file(path)?;
        j.extend(&args.jumps);
        let n = j.len();
        (j, format!("{n} chutes/ladders from {path}"))
    } else if !args.jumps.is_empty() {
        (args.jumps.clone(), format!("{} chutes/ladders", args.jumps.len()))
    } else {
        if args.squares < 100 {
            return Err("the standard board needs at least 100 squares; pass --jump, --board, or --empty".into());
        }
        (MILTON_BRADLEY.to_vec(), "standard Milton Bradley board".to_string())
    };

    let board = Board::new(args.squares, args.die, args.overshoot, jumps)?;
    let exact = markov::solve(&board)?;

    println!(
        "{} squares, d{}, overshoot: {:?}, {label}\n",
        board.last, board.die, board.overshoot
    );
    let fmt = |up: bool| -> String {
        let v: Vec<String> = board
            .jumps
            .iter()
            .filter(|(a, b)| (b > a) == up)
            .map(|(a, b)| format!("{a}->{b}"))
            .collect();
        if v.is_empty() { "none".into() } else { v.join(" ") }
    };
    if !board.jumps.is_empty() {
        println!("Ladders: {}\nChutes:  {}\n", fmt(true), fmt(false));
    }

    let sim = (args.games > 0).then(|| {
        let seed = args.seed.unwrap_or_else(rand::random);
        (sim::run(&board, args.games, seed), seed)
    });

    let col = |s: Option<String>| s.unwrap_or_default();
    match &sim {
        Some((s, seed)) => println!(
            "{:<22}{:>12}   simulated ({} games, seed {seed})",
            "", "exact", thousands(s.games)
        ),
        None => println!("{:<22}{:>12}", "", "exact"),
    }
    let sim_ref = sim.as_ref().map(|(s, _)| s);
    println!(
        "{:<22}{:>12.4}   {}",
        "Average turns",
        exact.expected_turns,
        col(sim_ref.map(|s| format!("{:.4}", s.mean_turns)))
    );
    println!(
        "{:<22}{:>12.4}   {}",
        "Std deviation",
        exact.std_dev,
        col(sim_ref.map(|s| format!("{:.4}", s.std_dev)))
    );
    println!(
        "{:<22}{:>12}   {}",
        "Shortest game",
        exact.shortest(),
        col(sim_ref.map(|s| s.shortest.to_string()))
    );
    println!(
        "{:<22}{:>12}",
        "Median / 90% / 99%",
        format!("{} / {} / {}", exact.quantile(0.5), exact.quantile(0.9), exact.quantile(0.99))
    );
    if let Some(s) = sim_ref {
        println!("{:<22}{:>12}   {}", "Longest game seen", "", s.longest);
        let (sq, gap) = (0..=board.last)
            .filter(|&q| !board.is_jump_start(q))
            .map(|q| (q, (exact.visits[q] - s.visits[q]).abs()))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        println!("\nLargest per-square gap between methods: {gap:.4} landings (square {sq})");
    }

    if args.per_square {
        println!("\nExpected landings per game:\n");
        println!("{:>6}  {:>10}  {:>10}", "square", "exact", if sim.is_some() { "simulated" } else { "" });
        for q in 0..=board.last {
            if board.is_jump_start(q) {
                continue;
            }
            println!(
                "{q:>6}  {:>10.4}  {}",
                exact.visits[q],
                col(sim_ref.map(|s| format!("{:>10.4}", s.visits[q])))
            );
        }
    }

    if let Some(path) = &args.matrix {
        let n = exact.fundamental_matrix();
        let mut f = std::io::BufWriter::new(fs::File::create(path)?);
        let header: Vec<String> = exact.states.iter().map(|s| s.to_string()).collect();
        writeln!(f, "from\\to,{}", header.join(","))?;
        for (i, row) in n.iter().enumerate() {
            let cells: Vec<String> = row.iter().map(|x| format!("{x:.6}")).collect();
            writeln!(f, "{},{}", exact.states[i], cells.join(","))?;
        }
        println!("\nWrote {}x{} fundamental matrix to {path}", n.len(), n.len());
    }
    Ok(())
}

fn read_board_file(path: &str) -> Result<Vec<(usize, usize)>, Box<dyn Error>> {
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut out = vec![];
    for (i, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        out.push(board::parse_jump(line).map_err(|e| format!("{path} line {}: {e}", i + 1))?);
    }
    Ok(out)
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
