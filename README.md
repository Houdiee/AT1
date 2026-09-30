# stable-match — Gale-Shapley stable matching

Programming Assignment 1, Track B. A Rust implementation of the Gale-Shapley
algorithm plus a command-line tool that matches two groups from plain-text
preference files, explains what it did, and independently verifies the result.

No external crates; `std` only.

## Build and run

```sh
nix-shell          # or just have cargo on PATH
cargo build --release
cargo test         # 9 tests, including 400 random instances

cargo run -- data/students.txt data/projects.txt --verify
```

## What it does

Given two groups where everyone ranks members of the other group, it finds a
**stable** matching: one where no unmatched pair would both rather have each
other than their assigned partners. Such a pair is called a *blocking pair*,
and a matching with none of them is stable. This is the algorithm behind real
systems like the US medical residency match.

## Input format

One line per member, `name: first choice, second choice, ...`. Blank lines and
`#` comments are ignored.

```
ana:   compiler, graphics, robotics
ben:   graphics, robotics, compiler
```

Preference lists may be **partial** — an unranked pair is treated as mutually
unacceptable — and the two groups need **not be the same size**, in which case
some members go unmatched.

## Options

| Option | Effect |
| --- | --- |
| `--verify` | Independently check the result has no blocking pair. Exits non-zero if it does. |
| `--compare` | Run it from both sides and show what each side pays for not proposing. |
| `--trace` | Print every proposal, acceptance and rejection as it happens. |
| `--propose-from <side>` | `proposers` (default) or `receivers`. Changes the answer — see below. |
| `--format <fmt>` | `text` (default) or `csv`, for piping into something else. |

## Example

```
$ cargo run -- data/applicants.txt data/internships.txt --trace --verify
  jo proposes to acme -> accepted (acme was free)
  kai proposes to bolt -> accepted (bolt was free)
  lena proposes to acme -> accepted (acme drops jo)
  jo proposes to bolt -> rejected (bolt prefers kai)
  jo has no choices left and stays unmatched
  mo proposes to crux -> accepted (crux was free)
  nina proposes to bolt -> rejected (bolt prefers kai)
  nina proposes to crux -> rejected (crux prefers mo)
  nina proposes to acme -> rejected (acme did not rank nina)
  nina has no choices left and stays unmatched

  jo    ->  (unmatched)
  kai   ->  bolt
  lena  ->  acme
  mo    ->  crux
  nina  ->  (unmatched)

  3/5 matched in 8 proposals
  verified stable: no blocking pairs
```

### Proposing is an advantage

`data/students.txt` is built so the two sides disagree as much as possible:
each project's favourite student is the one who ranked it last.

```
$ cargo run -- data/students.txt data/projects.txt --compare

  Average choice each side received (1.00 = everyone got their first pick):

                        students  projects
  students propose          1.00      6.00
  projects propose          6.00      1.00
```

Both matchings are stable. Whoever proposes gets their best stable matching and
the other side gets their worst — which is why, in a real matching scheme, the
choice of who proposes is a political decision, not a technical one.

## Layout

| Path | What it is |
| --- | --- |
| `src/lib.rs` | The algorithm (`gale_shapley`) and the stability checker (`blocking_pairs`), plus the tests. |
| `src/main.rs` | CLI: argument parsing, preference-file parsing and validation, output formatting. |
| `data/` | Two sample instances. |
| `report/report.docx` | The written report (submission copy). |
| `report/video_showcase_compressed.mp4` | The video showcase |

The algorithm itself is about 100 lines in the middle of `src/lib.rs` (roughly
half of that being the optional `--trace` messages); most of
the rest of the code is turning messy text files into indices and giving useful
errors when they do not line up.
