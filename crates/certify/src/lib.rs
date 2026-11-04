//! Situation-closure certificate for the monotile gliders (paper §7 /
//! FINDINGS §10.6): the immortality proof reduces to a finite closure
//! computation, and its one load-bearing input is a *complete* set of
//! legal collar extensions. This crate produces that input exactly.
//!
//! Two layers:
//!
//! 1. [`atlas`] — the **legal ball atlas**: every legal ball of edge
//!    radius R around a tile of each base type, enumerated by reading a
//!    tile address one symbol at a time through the neighbour
//!    transducer's DFA. A configuration is the partially resolved ball
//!    together with the DFA state of every still-pending neighbour
//!    query and the current top type; its future depends on nothing
//!    else, so configurations are memoised on exactly that and the
//!    exploration terminates. Every leaf is a legal ball (its witness is
//!    a legal finite address), and every legal ball is a leaf (every
//!    legal patch occurs inside some finite supertile, whose address
//!    path is explored). Termination is the completeness proof.
//!
//! 2. [`closure`] — the **situation closure**: states are canonical
//!    situation keys (pattern plus plain k-collar: classes and intrinsic
//!    edge gluings); transitions come from every atlas ball that extends
//!    the key around its centre cell. A closed, death-free set is the
//!    certificate.
//!
//! [`ball`] and [`situation`] hold the encoders shared by the atlas,
//! the closure and the validation against library-generated patches
//! ([`patchctx`], [`validate`]).

pub mod analysis;
pub mod atlas;
pub mod ball;
pub mod closure;
pub mod ctx;
pub mod dfa;
pub mod exact;
pub mod figdump;
pub mod param;
pub mod patchctx;
pub mod situation;
pub mod validate;
pub mod walk;
