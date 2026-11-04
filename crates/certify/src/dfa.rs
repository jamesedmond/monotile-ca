//! Driving the neighbour transducer's DFA directly (its `start`,
//! `accept` and `trans` are public). A *run* is one neighbour query
//! `(tile, edge)`: it consumes the tile's address symbols in order and
//! emits the neighbour's address symbols; on acceptance the two tiles'
//! common supertile has been found, the output has caught up with the
//! input, and every further input symbol is copied to the output.

use std::collections::HashSet;

use substitution_tiling_transducers::address::EdgeAddrSymbol;
use substitution_tiling_transducers::combinatorial::Transducer;
use substitution_tiling_transducers::common::TileIndex;

#[derive(Clone, Debug)]
pub struct Run {
    pub tile: u32,
    pub edge: u8,
    pub state: usize,
    pub out: Vec<EdgeAddrSymbol>,
    pub fed: usize,
}

impl Run {
    /// Start a query from a tile of type `ty` along `edge`; consumes the
    /// initial symbol immediately.
    pub fn new(tr: &Transducer, tile: u32, edge: u8, ty: TileIndex) -> Self {
        let mut run = Self {
            tile,
            edge,
            state: tr.start,
            out: Vec::new(),
            fed: 0,
        };
        run.feed(
            tr,
            EdgeAddrSymbol::Initial {
                base_type: ty,
                edge: edge as usize,
            },
        );
        run
    }

    pub fn feed(&mut self, tr: &Transducer, sym: EdgeAddrSymbol) {
        let (next, out) = tr.trans[&self.state]
            .get(&sym)
            .unwrap_or_else(|| panic!("illegal symbol {sym:?} in DFA state {}", self.state));
        self.state = *next;
        self.out.extend_from_slice(out);
        self.fed += 1;
    }

    pub fn accepted(&self, tr: &Transducer) -> bool {
        tr.accept.contains(&self.state)
    }
}

/// Structural facts about a transducer that the atlas model relies on,
/// checked rather than assumed: non-accepting transitions emit nothing
/// (so a pending query carries no buffered output), accepting
/// transitions emit exactly one symbol (copying), and every accepting
/// state only leads to accepting states.
pub struct DfaFacts {
    pub states: usize,
    pub accepting: usize,
    pub transitions: usize,
    pub pending_emit_transitions: usize,
    pub accepting_noncopy_transitions: usize,
    pub accepting_leaves_accept: usize,
    /// Distinct output lengths on transitions that enter an accepting
    /// state from a non-accepting one.
    pub burst_lengths: Vec<usize>,
}

pub fn dfa_facts(tr: &Transducer) -> DfaFacts {
    let accept: &HashSet<usize> = &tr.accept;
    let mut pending_emit = 0;
    let mut noncopy = 0;
    let mut leaves = 0;
    let mut bursts = HashSet::new();
    let mut transitions = 0;
    for (&s, table) in &tr.trans {
        for (next, out) in table.values() {
            transitions += 1;
            if accept.contains(&s) {
                if out.len() != 1 {
                    noncopy += 1;
                }
                if !accept.contains(next) {
                    leaves += 1;
                }
            } else if accept.contains(next) {
                bursts.insert(out.len());
            } else if !out.is_empty() {
                pending_emit += 1;
            }
        }
    }
    let mut burst_lengths: Vec<usize> = bursts.into_iter().collect();
    burst_lengths.sort_unstable();
    DfaFacts {
        states: tr.trans.len(),
        accepting: accept.len(),
        transitions,
        pending_emit_transitions: pending_emit,
        accepting_noncopy_transitions: noncopy,
        accepting_leaves_accept: leaves,
        burst_lengths,
    }
}
