//! Exact arithmetic for the cocycle: the tilings' vertex coordinates lie
//! in Z[ζ₁₂] (ζ = e^{iπ/6}), so translations recovered from the float
//! placements are exact ring elements, rotations by 30° multiples are
//! multiplications by powers of ζ, and the along/perpendicular
//! components of an edge vector relative to an exact reference direction
//! are elements of (Z + Z√3)/2, an ordered field where every comparison
//! is exact. The minimum-cycle-mean bounds then become computer-algebra
//! statements.
//!
//! The ring operations are named methods (`add`, `sub`, `mul`, `neg`) rather than the
//! `std::ops` traits, so that call sites read as exact ring arithmetic.
#![allow(clippy::should_implement_trait)]

use std::cmp::Ordering;

/// c0 + c1 ζ + c2 ζ² + c3 ζ³ with ζ⁴ = ζ² − 1.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Zeta(pub [i128; 4]);

impl Zeta {
    pub const ZERO: Zeta = Zeta([0; 4]);
    pub const ONE: Zeta = Zeta([1, 0, 0, 0]);
    pub const Z: Zeta = Zeta([0, 1, 0, 0]);

    pub fn add(self, o: Zeta) -> Zeta {
        Zeta([self.0[0] + o.0[0], self.0[1] + o.0[1], self.0[2] + o.0[2], self.0[3] + o.0[3]])
    }
    pub fn sub(self, o: Zeta) -> Zeta {
        Zeta([self.0[0] - o.0[0], self.0[1] - o.0[1], self.0[2] - o.0[2], self.0[3] - o.0[3]])
    }
    pub fn mul(self, o: Zeta) -> Zeta {
        let mut p = [0i128; 7];
        for i in 0..4 {
            for j in 0..4 {
                p[i + j] += self.0[i] * o.0[j];
            }
        }
        // x⁶ = −1, x⁵ = x³ − x, x⁴ = x² − 1.
        let (c6, c5, c4) = (p[6], p[5], p[4]);
        Zeta([p[0] - c4 - c6, p[1] - c5, p[2] + c4, p[3] + c5])
    }
    pub fn conj(self) -> Zeta {
        let [c0, c1, c2, c3] = self.0;
        Zeta([c0 + c2, c1, -c2, -c1 - c3])
    }
    /// ζ^k.
    pub fn power(k: u32) -> Zeta {
        let mut z = Zeta::ONE;
        for _ in 0..(k % 12) {
            z = z.mul(Zeta::Z);
        }
        z
    }
    /// Real and imaginary parts as elements of (Z + Z√3)/2.
    pub fn re(self) -> Half {
        let [c0, c1, c2, _] = self.0;
        Half { p: 2 * c0 + c2, q: c1 }
    }
    pub fn im(self) -> Half {
        let [_, c1, c2, c3] = self.0;
        Half { p: c1 + 2 * c3, q: c2 }
    }
    pub fn to_float(self) -> [f64; 2] {
        let r = self.re();
        let i = self.im();
        [r.to_float(), i.to_float()]
    }
    /// Exact ring element from a float point whose coordinates are of
    /// the form (a + b√3)/2 with bounded integers — unique within
    /// tolerance for the magnitudes that occur (|a|, |b| ≲ 10⁴).
    pub fn from_float(v: [f64; 2]) -> Option<Zeta> {
        let (a, b) = half_from_float(v[0])?;
        let (c, d) = half_from_float(v[1])?;
        // Re = ((2c0 + c2) + c1√3)/2, Im = ((c1 + 2c3) + c2√3)/2.
        let c1 = b;
        let c2 = d;
        if (a - c2) % 2 != 0 || (c - c1) % 2 != 0 {
            return None;
        }
        let c0 = (a - c2) / 2;
        let c3 = (c - c1) / 2;
        let z = Zeta([c0, c1, c2, c3]);
        let back = z.to_float();
        ((back[0] - v[0]).abs() < 1e-7 && (back[1] - v[1]).abs() < 1e-7).then_some(z)
    }
}

fn half_from_float(x: f64) -> Option<(i128, i128)> {
    let s3 = 3f64.sqrt();
    let two_x = 2.0 * x;
    let bmax = (two_x.abs() / s3).ceil() as i128 + 3;
    let mut found = None;
    for b in -bmax..=bmax {
        let a = (two_x - b as f64 * s3).round();
        if (two_x - a - b as f64 * s3).abs() < 1e-7 {
            if found.is_some() {
                return None; // ambiguous — should not happen at these magnitudes
            }
            found = Some((a as i128, b));
        }
    }
    found
}

/// (p + q√3)/2, an element of the ordered field Q(√3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Half {
    pub p: i128,
    pub q: i128,
}

impl Half {
    pub const ZERO: Half = Half { p: 0, q: 0 };
    pub fn add(self, o: Half) -> Half {
        Half { p: self.p + o.p, q: self.q + o.q }
    }
    pub fn sub(self, o: Half) -> Half {
        Half { p: self.p - o.p, q: self.q - o.q }
    }
    pub fn neg(self) -> Half {
        Half { p: -self.p, q: -self.q }
    }
    pub fn scale(self, k: i128) -> Half {
        Half { p: self.p * k, q: self.q * k }
    }
    pub fn to_float(self) -> f64 {
        (self.p as f64 + self.q as f64 * 3f64.sqrt()) / 2.0
    }
    /// Sign of p + q√3, exactly.
    pub fn sign(self) -> Ordering {
        let (p, q) = (self.p, self.q);
        match (p.cmp(&0), q.cmp(&0)) {
            (Ordering::Equal, s) | (s, Ordering::Equal) => s,
            (Ordering::Greater, Ordering::Greater) => Ordering::Greater,
            (Ordering::Less, Ordering::Less) => Ordering::Less,
            (Ordering::Greater, Ordering::Less) => (p * p).cmp(&(3 * q * q)),
            (Ordering::Less, Ordering::Greater) => (3 * q * q).cmp(&(p * p)),
        }
    }
    pub fn exact_cmp(self, o: Half) -> Ordering {
        self.sub(o).sign()
    }
}

/// Karp's minimum cycle mean with exact weights: returns (numerator,
/// denominator) with the mean = numerator / denominator (denominator a
/// positive integer, numerator a `Half`).
pub fn karp_exact(n: usize, edges: &[(usize, usize)], w: &[Half]) -> (Half, i128) {
    let mut dist: Vec<Vec<Option<Half>>> = vec![vec![None; n]; n + 1];
    dist[0][0] = Some(Half::ZERO);
    for k in 1..=n {
        for (i, &(u, v)) in edges.iter().enumerate() {
            if let Some(du) = dist[k - 1][u] {
                let cand = du.add(w[i]);
                let better = match dist[k][v] {
                    None => true,
                    Some(cur) => cand.exact_cmp(cur) == Ordering::Less,
                };
                if better {
                    dist[k][v] = Some(cand);
                }
            }
        }
    }
    // Fractions a/m compared exactly by cross-multiplication.
    let less = |a: Half, ma: i128, b: Half, mb: i128| a.scale(mb).exact_cmp(b.scale(ma)) == Ordering::Less;
    let mut best: Option<(Half, i128)> = None;
    for v in 0..n {
        let Some(dn) = dist[n][v] else { continue };
        let mut worst: Option<(Half, i128)> = None;
        for (k, row) in dist.iter().enumerate().take(n) {
            if let Some(dk) = row[v] {
                let cand = (dn.sub(dk), (n - k) as i128);
                if worst.is_none_or(|(a, m)| less(a, m, cand.0, cand.1)) {
                    worst = Some(cand);
                }
            }
        }
        if let Some(wv) = worst
            && best.is_none_or(|(a, m)| less(wv.0, wv.1, a, m))
        {
            best = Some(wv);
        }
    }
    best.expect("strongly connected graph has a cycle")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeta_arithmetic_matches_floats() {
        let z = Zeta([3, -2, 5, 1]);
        let w = Zeta([-1, 4, 0, 2]);
        let prod = z.mul(w).to_float();
        let (zf, wf) = (z.to_float(), w.to_float());
        let expect = [zf[0] * wf[0] - zf[1] * wf[1], zf[0] * wf[1] + zf[1] * wf[0]];
        assert!((prod[0] - expect[0]).abs() < 1e-9 && (prod[1] - expect[1]).abs() < 1e-9);
        let c = z.conj().to_float();
        assert!((c[0] - zf[0]).abs() < 1e-9 && (c[1] + zf[1]).abs() < 1e-9);
        let r = Zeta::power(1).to_float();
        assert!((r[0] - 3f64.sqrt() / 2.0).abs() < 1e-12 && (r[1] - 0.5).abs() < 1e-12);
        assert_eq!(Zeta::from_float(zf), Some(z));
    }

    #[test]
    fn half_sign_is_exact() {
        assert_eq!(Half { p: 7, q: -4 }.sign(), Ordering::Greater); // 7 − 6.928 > 0
        assert_eq!(Half { p: -7, q: 4 }.sign(), Ordering::Less);
        assert_eq!(Half { p: 5, q: -3 }.sign(), Ordering::Less); // 5 − 5.196 < 0
        assert_eq!(Half { p: 0, q: 0 }.sign(), Ordering::Equal);
    }
}
