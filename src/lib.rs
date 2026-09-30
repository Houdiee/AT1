//! Gale-Shapley stable matching.
//!
//! Given two groups -- here called *proposers* and *receivers* -- where every
//! member ranks members of the other group, a matching is **stable** if no
//! unmatched pair would both rather have each other than their current
//! partners. Such a pair is called a *blocking pair*. Gale and Shapley (1962)
//! showed a stable matching always exists and gave this algorithm to find one.
//!
//! The whole algorithm is `gale_shapley`; everything else in this file is
//! bookkeeping around it or the independent stability checker used by
//! `stable-match --verify`.

use std::collections::HashMap;

/// A matching problem: two named groups and their preference lists.
///
/// Preference lists may be *partial* -- you do not have to rank everyone. An
/// unranked pair is treated as mutually unacceptable, which lets the tool
/// handle real input where, say, a student only applies to four projects.
/// Groups may also be different sizes, in which case some members go unmatched.
pub struct Instance {
    pub proposers: Vec<String>,
    pub receivers: Vec<String>,
    /// `proposer_prefs[p]` lists receiver indices, most preferred first.
    proposer_prefs: Vec<Vec<usize>>,
    receiver_prefs: Vec<Vec<usize>>,
    /// `proposer_rank[p][r]` is r's position in p's list, or `None` if p did
    /// not rank r. Precomputed so "does p prefer r to q?" is an array lookup
    /// and a comparison rather than a scan.
    proposer_rank: Vec<Vec<Option<usize>>>,
    receiver_rank: Vec<Vec<Option<usize>>>,
}

/// `rank[target] = position of target in prefs`, for a group of `n_targets`.
fn rank_table(n_targets: usize, prefs: &[Vec<usize>]) -> Vec<Vec<Option<usize>>> {
    prefs
        .iter()
        .map(|list| {
            let mut rank = vec![None; n_targets];
            for (position, &target) in list.iter().enumerate() {
                rank[target] = Some(position);
            }
            rank
        })
        .collect()
}

impl Instance {
    pub fn new(
        proposers: Vec<String>,
        receivers: Vec<String>,
        proposer_prefs: Vec<Vec<usize>>,
        receiver_prefs: Vec<Vec<usize>>,
    ) -> Instance {
        let proposer_rank = rank_table(receivers.len(), &proposer_prefs);
        let receiver_rank = rank_table(proposers.len(), &receiver_prefs);
        Instance {
            proposers,
            receivers,
            proposer_prefs,
            receiver_prefs,
            proposer_rank,
            receiver_rank,
        }
    }

    /// Position (0-based) of receiver `r` in proposer `p`'s preference list,
    /// or `None` if p did not rank r.
    pub fn proposer_position(&self, p: usize, r: usize) -> Option<usize> {
        self.proposer_rank[p][r]
    }

    /// Position (0-based) of proposer `p` in receiver `r`'s preference list,
    /// or `None` if r did not rank p.
    pub fn receiver_position(&self, r: usize, p: usize) -> Option<usize> {
        self.receiver_rank[r][p]
    }

    /// The same problem with the two sides exchanged. Running the algorithm on
    /// this gives the *receiver*-optimal matching instead of the
    /// proposer-optimal one, which is how `--propose-from` is implemented.
    pub fn flipped(&self) -> Instance {
        Instance::new(
            self.receivers.clone(),
            self.proposers.clone(),
            self.receiver_prefs.clone(),
            self.proposer_prefs.clone(),
        )
    }
}

pub struct Matching {
    pub partner_of_proposer: Vec<Option<usize>>,
    pub partner_of_receiver: Vec<Option<usize>>,
    /// Total proposals made. Bounded by the total length of all preference
    /// lists, which is what makes the loop terminate.
    pub proposals: usize,
    /// Human-readable trace, empty unless `trace` was set.
    pub log: Vec<String>,
}

/// Run Gale-Shapley and return the proposer-optimal stable matching.
///
/// # Invariant
///
/// **Once a receiver is holding a proposal they never become unmatched again,
/// and the proposer they hold only ever improves in their ranking.**
///
/// It is established in the two places `partner_of_receiver[r]` is written:
/// the `None` arm assigns a first partner and nothing ever writes `None` back,
/// and the `Some(held)` arm overwrites only when `rank_new < rank_held`. The
/// whole correctness argument leans on it -- see the stability proof sketch in
/// `report/report.md`.
pub fn gale_shapley(inst: &Instance, trace: bool) -> Matching {
    let n_proposers = inst.proposers.len();
    let n_receivers = inst.receivers.len();

    let mut partner_of_receiver: Vec<Option<usize>> = vec![None; n_receivers];
    // `next[p]` counts how many of p's choices have already been proposed to,
    // so it doubles as the index of p's next choice.
    let mut next = vec![0usize; n_proposers];
    // Everyone starts free. Popping from the back of a stack is arbitrary --
    // the final matching is the same whatever order free proposers are served
    // in, which is itself a non-obvious property of this algorithm.
    let mut free: Vec<usize> = (0..n_proposers).rev().collect();

    let mut proposals = 0;
    let mut log = Vec::new();

    while let Some(p) = free.pop() {
        let prefs = &inst.proposer_prefs[p];
        if next[p] == prefs.len() {
            // p has been rejected by everyone they ranked, so p stays
            // unmatched. Without partial lists this branch is unreachable.
            if trace {
                log.push(format!(
                    "{} has no choices left and stays unmatched",
                    inst.proposers[p]
                ));
            }
            continue;
        }

        let r = prefs[next[p]];
        // Advancing `next[p]` here is what guarantees termination: it makes
        // every iteration consume one entry of one preference list, so the loop
        // runs at most (total list length) times. Remove it and a rejected
        // proposer re-proposes to the same receiver forever.
        proposals += 1;

        let Some(rank_p) = inst.receiver_rank[r][p] else {
            // r did not rank p at all, so the pair is unacceptable to r.
            if trace {
                log.push(format!(
                    "{} proposes to {} -> rejected ({} did not rank {})",
                    inst.proposers[p], inst.receivers[r], inst.receivers[r], inst.proposers[p]
                ));
            }
            free.push(p);
            continue;
        };

        match partner_of_receiver[r] {
            None => {
                partner_of_receiver[r] = Some(p);
                if trace {
                    log.push(format!(
                        "{} proposes to {} -> accepted ({} was free)",
                        inst.proposers[p], inst.receivers[r], inst.receivers[r]
                    ));
                }
            }
            Some(held) => {
                let rank_held = inst.receiver_rank[r][held]
                    .expect("a held proposer was ranked by this receiver");
                if rank_p < rank_held {
                    // Strict improvement for r. Changing `<` to `<=` here would
                    // let r swap between equally-ranked proposers -- impossible
                    // with strict preferences, but it would also break the
                    // "only improves" invariant if ties were ever allowed.
                    partner_of_receiver[r] = Some(p);
                    free.push(held);
                    if trace {
                        log.push(format!(
                            "{} proposes to {} -> accepted ({} drops {})",
                            inst.proposers[p],
                            inst.receivers[r],
                            inst.receivers[r],
                            inst.proposers[held]
                        ));
                    }
                } else {
                    free.push(p);
                    if trace {
                        log.push(format!(
                            "{} proposes to {} -> rejected ({} prefers {})",
                            inst.proposers[p],
                            inst.receivers[r],
                            inst.receivers[r],
                            inst.proposers[held]
                        ));
                    }
                }
            }
        }
    }

    let mut partner_of_proposer = vec![None; n_proposers];
    for (r, &holder) in partner_of_receiver.iter().enumerate() {
        if let Some(p) = holder {
            partner_of_proposer[p] = Some(r);
        }
    }

    Matching {
        partner_of_proposer,
        partner_of_receiver,
        proposals,
        log,
    }
}

/// Every blocking pair in `m`: pairs `(p, r)` who each rank the other above
/// their assigned partner, and would therefore both defect.
///
/// This is deliberately written from the *definition* of stability rather than
/// by reusing anything from `gale_shapley`, so that `--verify` is an
/// independent check on the algorithm and not a restatement of it. A correct
/// run always returns an empty vector.
pub fn blocking_pairs(inst: &Instance, m: &Matching) -> Vec<(usize, usize)> {
    let mut found = Vec::new();

    for p in 0..inst.proposers.len() {
        // Only pairs p ranked can block: an unranked r is unacceptable to p.
        for &r in &inst.proposer_prefs[p] {
            let p_prefers_r = match m.partner_of_proposer[p] {
                None => true, // unmatched, so any acceptable partner is better
                Some(current) => {
                    let (rank_r, rank_current) = (
                        inst.proposer_rank[p][r].expect("r comes from p's own list"),
                        inst.proposer_rank[p][current]
                            .expect("p only proposes to ranked receivers"),
                    );
                    rank_r < rank_current
                }
            };
            if !p_prefers_r {
                continue;
            }

            let Some(rank_p) = inst.receiver_rank[r][p] else {
                continue; // r finds p unacceptable, so they cannot block
            };
            let r_prefers_p = match m.partner_of_receiver[r] {
                None => true,
                Some(current) => {
                    let rank_current = inst.receiver_rank[r][current]
                        .expect("a matched proposer was ranked by this receiver");
                    rank_p < rank_current
                }
            };
            if r_prefers_p {
                found.push((p, r));
            }
        }
    }
    found
}

/// Look a name up in a group, for turning parsed text into indices.
pub fn index_of(names: &[String]) -> HashMap<&str, usize> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an instance from raw index lists; names are just `p0`, `r0`, ...
    fn make(
        n_proposers: usize,
        n_receivers: usize,
        proposer_prefs: Vec<Vec<usize>>,
        receiver_prefs: Vec<Vec<usize>>,
    ) -> Instance {
        Instance::new(
            (0..n_proposers).map(|i| format!("p{i}")).collect(),
            (0..n_receivers).map(|i| format!("r{i}")).collect(),
            proposer_prefs,
            receiver_prefs,
        )
    }

    /// The classic instance where every proposer's first choice is distinct,
    /// so the proposer-optimal matching gives everyone their top pick.
    #[test]
    fn everyone_gets_their_first_choice_when_tops_are_distinct() {
        let inst = make(
            3,
            3,
            vec![vec![0, 1, 2], vec![1, 2, 0], vec![2, 0, 1]],
            vec![vec![1, 2, 0], vec![2, 0, 1], vec![0, 1, 2]],
        );
        let m = gale_shapley(&inst, false);
        assert_eq!(m.partner_of_proposer, vec![Some(0), Some(1), Some(2)]);
        assert_eq!(m.proposals, 3);
        assert!(blocking_pairs(&inst, &m).is_empty());
    }

    /// Proposing is an advantage: on this instance each receiver's favourite
    /// proposer is the one who ranked them last, so the two sides produce
    /// opposite matchings and both are stable.
    #[test]
    fn the_proposing_side_gets_its_best_stable_matching() {
        let n = 4;
        // proposer i ranks receivers i, i+1, i+2, ... cyclically.
        let proposer_prefs: Vec<Vec<usize>> = (0..n)
            .map(|i| (0..n).map(|j| (i + j) % n).collect())
            .collect();
        // receiver j ranks proposers j+1, j+2, ... so j's favourite is the
        // proposer for whom j is the *last* choice.
        let receiver_prefs: Vec<Vec<usize>> = (0..n)
            .map(|j| (0..n).map(|k| (j + 1 + k) % n).collect())
            .collect();

        let inst = make(n, n, proposer_prefs, receiver_prefs);
        let forward = gale_shapley(&inst, false);
        let backward = gale_shapley(&inst.flipped(), false);

        // Proposers get their first choice; receivers get their last.
        assert_eq!(
            forward.partner_of_proposer,
            vec![Some(0), Some(1), Some(2), Some(3)]
        );
        // Flipped, the receivers get their first choice instead.
        assert_eq!(
            backward.partner_of_proposer,
            vec![Some(1), Some(2), Some(3), Some(0)]
        );

        // Both are stable -- they are just different stable matchings.
        assert!(blocking_pairs(&inst, &forward).is_empty());
        assert!(blocking_pairs(&inst.flipped(), &backward).is_empty());
    }

    #[test]
    fn partial_lists_leave_people_unmatched_but_stable() {
        // p0 will only accept r0; r0 prefers p1, who also wants r0.
        let inst = make(
            2,
            2,
            vec![vec![0], vec![0, 1]],
            vec![vec![1, 0], vec![1, 0]],
        );
        let m = gale_shapley(&inst, false);
        assert_eq!(m.partner_of_proposer[1], Some(0), "p1 wins r0");
        assert_eq!(m.partner_of_proposer[0], None, "p0 ranked nobody else");
        assert!(blocking_pairs(&inst, &m).is_empty());
    }

    #[test]
    fn unequal_group_sizes() {
        // 3 proposers, 2 receivers: exactly one proposer must go unmatched.
        let inst = make(
            3,
            2,
            vec![vec![0, 1], vec![0, 1], vec![0, 1]],
            vec![vec![0, 1, 2], vec![0, 1, 2]],
        );
        let m = gale_shapley(&inst, false);
        let matched = m.partner_of_proposer.iter().filter(|p| p.is_some()).count();
        assert_eq!(matched, 2);
        assert!(blocking_pairs(&inst, &m).is_empty());
    }

    #[test]
    fn unacceptable_pairs_are_never_matched() {
        // Nobody ranks anybody, so nothing can be matched.
        let inst = make(2, 2, vec![vec![], vec![]], vec![vec![], vec![]]);
        let m = gale_shapley(&inst, false);
        assert_eq!(m.partner_of_proposer, vec![None, None]);
        assert_eq!(m.proposals, 0);
        assert!(blocking_pairs(&inst, &m).is_empty());
    }

    /// A receiver must reject a proposer they did not rank, even when free.
    #[test]
    fn a_free_receiver_still_rejects_an_unranked_proposer() {
        let inst = make(1, 1, vec![vec![0]], vec![vec![]]);
        let m = gale_shapley(&inst, false);
        assert_eq!(m.partner_of_proposer, vec![None]);
        assert_eq!(m.partner_of_receiver, vec![None]);
    }

    /// The verifier must not be vacuous: give it a hand-built unstable
    /// matching and check it actually complains. Without this test,
    /// `--verify` passing would prove nothing.
    #[test]
    fn the_verifier_detects_an_unstable_matching() {
        // Both proposers prefer r0; both receivers prefer p0. Pairing p0 with
        // r1 and p1 with r0 is unstable: p0 and r0 would both rather swap.
        let inst = make(
            2,
            2,
            vec![vec![0, 1], vec![0, 1]],
            vec![vec![0, 1], vec![0, 1]],
        );
        let bad = Matching {
            partner_of_proposer: vec![Some(1), Some(0)],
            partner_of_receiver: vec![Some(1), Some(0)],
            proposals: 0,
            log: Vec::new(),
        };
        assert_eq!(blocking_pairs(&inst, &bad), vec![(0, 0)]);

        // ...and the real algorithm does not produce it.
        let good = gale_shapley(&inst, false);
        assert!(blocking_pairs(&inst, &good).is_empty());
        assert_eq!(good.partner_of_proposer[0], Some(0));
    }

    /// A tiny deterministic generator, only used to build random instances.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn permutation(&mut self, n: usize) -> Vec<usize> {
            let mut v: Vec<usize> = (0..n).collect();
            for i in (1..n).rev() {
                let j = (self.next() as usize) % (i + 1);
                v.swap(i, j);
            }
            v
        }
    }

    /// The property that actually matters, checked on many random instances:
    /// the result is always a valid matching with no blocking pair.
    #[test]
    fn random_instances_are_always_stable() {
        let mut rng = Lcg(12345);
        for n in 2..12 {
            for _ in 0..40 {
                let proposer_prefs: Vec<Vec<usize>> = (0..n).map(|_| rng.permutation(n)).collect();
                let receiver_prefs: Vec<Vec<usize>> = (0..n).map(|_| rng.permutation(n)).collect();
                let inst = make(n, n, proposer_prefs, receiver_prefs);
                let m = gale_shapley(&inst, false);

                // Complete: with full preference lists everyone is matched.
                assert!(m.partner_of_proposer.iter().all(|p| p.is_some()));
                // Consistent: the two directions agree.
                for (p, partner) in m.partner_of_proposer.iter().enumerate() {
                    assert_eq!(m.partner_of_receiver[partner.unwrap()], Some(p));
                }
                // Stable.
                assert!(blocking_pairs(&inst, &m).is_empty(), "n={n}");
                // Terminating: at most n^2 proposals were needed.
                assert!(m.proposals <= n * n);
            }
        }
    }

    #[test]
    fn trace_is_empty_unless_requested() {
        let inst = make(1, 1, vec![vec![0]], vec![vec![0]]);
        assert!(gale_shapley(&inst, false).log.is_empty());
        assert!(!gale_shapley(&inst, true).log.is_empty());
    }
}
