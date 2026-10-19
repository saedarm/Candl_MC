# candl

Chutes and Ladders, two ways, in one Rust binary:

1. **Simulate it.** Play the game millions of times (in parallel) and count.
2. **Solve it exactly.** Treat the board as an absorbing Markov chain and get the
   answer from the fundamental matrix N = (I − Q)⁻¹.

Then print both side by side. If they disagree, something is wrong with the board
or the code.

This is a rewrite of the original C++ simulator and Python fundamental-matrix script
from [saedarm/Candl_MC](https://github.com/saedarm/Candl_MC), which did the two
halves separately with separate board setup.

## Run it

```
cargo run --release
```

```
100 squares, d6, overshoot: Stay, standard Milton Bradley board

Ladders: 1->38 4->14 9->31 21->42 28->84 36->44 51->67 71->91 80->100
Chutes:  16->6 47->26 49->11 56->53 62->19 64->60 87->24 93->73 95->75 98->78

                             exact   simulated (10,000,000 games, seed 42)
Average turns              39.2251   39.2237
Std deviation              25.2250   25.2363
Shortest game                    7   7
Median / 90% / 99%    32 / 72 / 128
Longest game seen                    470
```

Ten million games take about a second.

## Options

| Flag | What it does |
|---|---|
| `-s, --squares N` | Playable squares (default 100). Square 0 is added for you, so no more "enter 101". |
| `-d, --die N` | Sides on the die (default 6). |
| `-j, --jump FROM:TO` | Add a chute or ladder. Repeatable. |
| `-b, --board FILE` | Read jumps from a file, one `FROM TO` per line. See `boards/`. |
| `--empty` | No chutes or ladders. |
| `--overshoot stay\|bounce` | Roll past the end: don't move (original rule), or bounce back. |
| `-g, --games N` | Games to simulate (default 1,000,000; `0` = exact only). |
| `--seed N` | Make a simulation run reproducible. |
| `--per-square` | Expected landings on every square, both methods. |
| `--matrix FILE` | Write the full fundamental matrix N as CSV. |

Examples:

```
candl --squares 30 -j 3:22 -j 5:8 -j 11:26 -j 20:29 -j 17:4 -j 19:7 -j 27:1 -j 21:9
candl --board boards/milton_bradley.txt --overshoot bounce --per-square
candl --die 4 --games 0 --matrix N.csv
```

## Rules

- Start off the board on square 0. Each turn, roll and move.
- Land on the start of a chute or ladder and you take it. If it drops you on
  another chute or ladder, you take that one too.
- A roll past the last square: `stay` doesn't move you, `bounce` bounces you back.
- The game ends when you land exactly on the last square.

## Board checks

Bad boards are rejected up front with a plain explanation:

- a jump off the board, starting on square 0, or starting on the final square
- two jumps starting on the same square
- jumps that loop into each other (`2:5` and `5:2`)
- boards where a player can get stuck forever. This is checked exactly from the
  chain, so there's no "too many turns" failsafe to tune.

## Layout

```
src/board.rs   board, validation, the movement rule (shared by both methods)
src/sim.rs     parallel Monte Carlo
src/markov.rs  transition matrix, LU solve, fundamental matrix, game-length distribution
src/main.rs    CLI and output
```
