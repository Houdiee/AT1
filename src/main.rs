//! Command-line interface for the stable matcher.

use stable_match::{blocking_pairs, gale_shapley, index_of, Instance, Matching};
use std::collections::HashSet;
use std::process::ExitCode;

const USAGE: &str = "\
stable-match -- find a stable matching between two groups (Gale-Shapley)

USAGE:
    stable-match <PROPOSERS> <RECEIVERS> [OPTIONS]

ARGS:
    <PROPOSERS>   preference file for the proposing side
    <RECEIVERS>   preference file for the receiving side

OPTIONS:
    --propose-from <side>   which side proposes: `proposers` (default) or
                            `receivers`. The proposing side gets the matching
                            that is best for it, so this changes the answer.
    --compare               run it both ways and show how much the proposing
                            side gains
    --verify                independently check the result has no blocking pair
    --trace                 print every proposal as it happens
    --format <fmt>          `text` (default) or `csv`
    -h, --help              print this help

FILE FORMAT:
    One line per member, `name: first choice, second choice, ...`.
    Blank lines and lines starting with `#` are ignored. Preference lists may
    be partial -- an unranked pair is treated as mutually unacceptable -- and
    the two groups need not be the same size.

EXAMPLE:
    stable-match data/students.txt data/projects.txt --verify
";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

struct Options {
    proposers_path: String,
    receivers_path: String,
    flip: bool,
    compare: bool,
    verify: bool,
    trace: bool,
    csv: bool,
}

fn parse_args() -> Result<Option<Options>, String> {
    let mut positional = Vec::new();
    let (mut flip, mut compare, mut verify, mut trace, mut csv) =
        (false, false, false, false, false);

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--verify" => verify = true,
            "--trace" => trace = true,
            "--compare" => compare = true,
            "--propose-from" => match args.next().as_deref() {
                Some("proposers") => flip = false,
                Some("receivers") => flip = true,
                Some(other) => {
                    return Err(format!(
                        "--propose-from expects `proposers` or `receivers`, got `{other}`"
                    ))
                }
                None => return Err("--propose-from needs a value".into()),
            },
            "--format" => match args.next().as_deref() {
                Some("text") => csv = false,
                Some("csv") => csv = true,
                Some(other) => {
                    return Err(format!("--format expects `text` or `csv`, got `{other}`"))
                }
                None => return Err("--format needs a value".into()),
            },
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}` (try --help)"))
            }
            other => positional.push(other.to_string()),
        }
    }

    if positional.len() != 2 {
        return Err(format!(
            "expected 2 preference files, got {} (try --help)",
            positional.len()
        ));
    }
    Ok(Some(Options {
        proposers_path: positional[0].clone(),
        receivers_path: positional[1].clone(),
        flip,
        compare,
        verify,
        trace,
        csv,
    }))
}

/// One parsed line: a member and the names they ranked, in order.
struct Entry {
    name: String,
    prefs: Vec<String>,
    line_number: usize,
}

fn parse_file(path: &str) -> Result<Vec<Entry>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut entries = Vec::new();

    for (index, raw) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = raw.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let Some((name, rest)) = line.split_once(':') else {
            return Err(format!(
                "{path}:{line_number}: expected `name: choices`, found `{line}`"
            ));
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(format!("{path}:{line_number}: missing name before `:`"));
        }
        let prefs: Vec<String> = rest
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        entries.push(Entry {
            name,
            prefs,
            line_number,
        });
    }

    if entries.is_empty() {
        return Err(format!("{path}: no entries found"));
    }
    Ok(entries)
}

/// Turn two parsed files into an `Instance`, rejecting anything inconsistent.
/// Most of the value of this tool is here rather than in the algorithm: real
/// preference files have typos, and a silent misspelling would produce a
/// plausible-looking wrong matching.
fn build_instance(
    proposer_entries: &[Entry],
    receiver_entries: &[Entry],
    proposers_path: &str,
    receivers_path: &str,
) -> Result<Instance, String> {
    let names = |entries: &[Entry]| entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
    let proposers = names(proposer_entries);
    let receivers = names(receiver_entries);

    check_unique(&proposers, proposers_path)?;
    check_unique(&receivers, receivers_path)?;

    let proposer_index = index_of(&proposers);
    let receiver_index = index_of(&receivers);

    let resolve = |entries: &[Entry],
                   lookup: &std::collections::HashMap<&str, usize>,
                   path: &str,
                   other: &str|
     -> Result<Vec<Vec<usize>>, String> {
        entries
            .iter()
            .map(|entry| {
                let mut seen = HashSet::new();
                entry
                    .prefs
                    .iter()
                    .map(|choice| {
                        let Some(&idx) = lookup.get(choice.as_str()) else {
                            return Err(format!(
                                "{}:{}: `{}` ranks `{}`, who is not listed in {}",
                                path, entry.line_number, entry.name, choice, other
                            ));
                        };
                        if !seen.insert(idx) {
                            return Err(format!(
                                "{}:{}: `{}` ranks `{}` more than once",
                                path, entry.line_number, entry.name, choice
                            ));
                        }
                        Ok(idx)
                    })
                    .collect()
            })
            .collect()
    };

    let proposer_prefs = resolve(
        proposer_entries,
        &receiver_index,
        proposers_path,
        receivers_path,
    )?;
    let receiver_prefs = resolve(
        receiver_entries,
        &proposer_index,
        receivers_path,
        proposers_path,
    )?;

    Ok(Instance::new(
        proposers,
        receivers,
        proposer_prefs,
        receiver_prefs,
    ))
}

fn check_unique(names: &[String], path: &str) -> Result<(), String> {
    let mut seen = HashSet::new();
    for name in names {
        if !seen.insert(name) {
            return Err(format!("{path}: `{name}` is listed twice"));
        }
    }
    Ok(())
}

/// Mean position of each proposer's partner in their own preference list.
/// 1.0 means everyone got their first choice. Unmatched members are skipped.
fn average_choice(prefs_rank: impl Iterator<Item = Option<usize>>) -> Option<f64> {
    let ranks: Vec<usize> = prefs_rank.flatten().collect();
    if ranks.is_empty() {
        return None;
    }
    Some(ranks.iter().map(|r| r + 1).sum::<usize>() as f64 / ranks.len() as f64)
}

fn report(inst: &Instance, m: &Matching, csv: bool) {
    if csv {
        println!("proposer,receiver");
        for (p, partner) in m.partner_of_proposer.iter().enumerate() {
            let target = partner.map_or("", |r| inst.receivers[r].as_str());
            println!("{},{}", inst.proposers[p], target);
        }
        return;
    }

    let width = inst.proposers.iter().map(|n| n.len()).max().unwrap_or(0);
    for (p, partner) in m.partner_of_proposer.iter().enumerate() {
        match partner {
            Some(r) => println!("  {:width$}  ->  {}", inst.proposers[p], inst.receivers[*r]),
            None => println!("  {:width$}  ->  (unmatched)", inst.proposers[p]),
        }
    }

    let matched = m.partner_of_proposer.iter().filter(|p| p.is_some()).count();
    println!(
        "\n  {matched}/{} matched in {} proposals",
        inst.proposers.len(),
        m.proposals
    );
}

fn run() -> Result<(), String> {
    let Some(opts) = parse_args()? else {
        print!("{USAGE}");
        return Ok(());
    };

    let proposer_entries = parse_file(&opts.proposers_path)?;
    let receiver_entries = parse_file(&opts.receivers_path)?;
    let base = build_instance(
        &proposer_entries,
        &receiver_entries,
        &opts.proposers_path,
        &opts.receivers_path,
    )?;

    if opts.compare {
        return compare(
            &base,
            &opts,
            &stem(&opts.proposers_path),
            &stem(&opts.receivers_path),
        );
    }

    // Flipping the instance and running the same algorithm is what gives the
    // receiver-optimal matching. Output is always phrased as
    // "whoever proposed -> whoever received".
    let flipped;
    let inst = if opts.flip {
        flipped = base.flipped();
        &flipped
    } else {
        &base
    };

    let m = gale_shapley(inst, opts.trace);

    if opts.trace {
        for line in &m.log {
            println!("  {line}");
        }
        println!();
    }

    report(inst, &m, opts.csv);

    if opts.verify {
        let blocking = blocking_pairs(inst, &m);
        if blocking.is_empty() {
            println!("  verified stable: no blocking pairs");
        } else {
            println!("\n  UNSTABLE -- {} blocking pair(s):", blocking.len());
            for (p, r) in &blocking {
                println!(
                    "    {} and {} would both defect",
                    inst.proposers[*p], inst.receivers[*r]
                );
            }
            return Err("matching is not stable".into());
        }
    }
    Ok(())
}

/// The file name without directory or extension, used to label the two sides
/// in `--compare` output.
fn stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Run the algorithm from both sides and show what each side pays for not
/// proposing. Both results are stable; they are just different stable
/// matchings, and each side's best stable outcome is the other side's worst.
fn compare(base: &Instance, opts: &Options, left: &str, right: &str) -> Result<(), String> {
    let flipped = base.flipped();
    let a = gale_shapley(base, false);
    let b = gale_shapley(&flipped, false);

    println!("When {left} propose:");
    report(base, &a, opts.csv);
    println!();
    println!("When {right} propose:");
    report(&flipped, &b, opts.csv);

    // For each run, the mean position of the partner each side received, in
    // that side's own preference list. Reporting only the proposing side would
    // hide the whole point -- what matters is what the other side pays.
    let averages = |inst: &Instance, m: &Matching| {
        let proposing = average_choice(
            m.partner_of_proposer
                .iter()
                .enumerate()
                .map(|(p, partner)| partner.and_then(|r| inst.proposer_position(p, r))),
        );
        let receiving = average_choice(
            m.partner_of_receiver
                .iter()
                .enumerate()
                .map(|(r, partner)| partner.and_then(|p| inst.receiver_position(r, p))),
        );
        (proposing, receiving)
    };

    // In run `b` the sides are swapped, so its proposing side is `right`.
    let (a_left, a_right) = averages(base, &a);
    let (b_right, b_left) = averages(&flipped, &b);

    let show = |v: Option<f64>| v.map_or("  -- ".to_string(), |x| format!("{x:.2}"));
    let w = left.len().max(right.len());
    println!("\n  Average choice each side received (1.00 = everyone got their first pick):\n");
    println!("  {:<w$}     {:>8}  {:>8}", "", left, right, w = w + 9);
    println!(
        "  {:<w$}     {:>8}  {:>8}",
        format!("{left} propose"),
        show(a_left),
        show(a_right),
        w = w + 9
    );
    println!(
        "  {:<w$}     {:>8}  {:>8}",
        format!("{right} propose"),
        show(b_left),
        show(b_right),
        w = w + 9
    );
    println!("\n  Whichever side proposes gets its best stable matching, and the");
    println!("  other side gets its worst.");
    Ok(())
}
