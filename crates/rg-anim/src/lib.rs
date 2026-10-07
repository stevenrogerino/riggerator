//! rg-anim — animation curves (Maya's animCurveTL/TA/TU family).
//!
//! Time is measured in frames. Curves hold keys with per-key in/out tangent
//! types; evaluation uses cubic Hermite segments like Maya's non-weighted
//! tangents.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tangent {
    /// Maya's default: smooth spline that never overshoots between keys.
    #[default]
    Auto,
    Spline,
    Clamped,
    Linear,
    Flat,
    Stepped,
    /// Explicit slope stored on the key (value units per frame).
    Fixed,
}

impl Tangent {
    pub const ALL: [Tangent; 7] =
        [Tangent::Auto, Tangent::Spline, Tangent::Clamped, Tangent::Linear, Tangent::Flat, Tangent::Stepped, Tangent::Fixed];
    pub fn name(self) -> &'static str {
        match self {
            Tangent::Auto => "auto",
            Tangent::Spline => "spline",
            Tangent::Clamped => "clamped",
            Tangent::Linear => "linear",
            Tangent::Flat => "flat",
            Tangent::Stepped => "step",
            Tangent::Fixed => "fixed",
        }
    }
    pub fn parse(s: &str) -> Option<Tangent> {
        Tangent::ALL.into_iter().find(|t| t.name() == s || (s == "stepped" && *t == Tangent::Stepped))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Infinity {
    #[default]
    Constant,
    Linear,
    Cycle,
    CycleRelative,
    Oscillate,
}

impl Infinity {
    pub const ALL: [Infinity; 5] =
        [Infinity::Constant, Infinity::Linear, Infinity::Cycle, Infinity::CycleRelative, Infinity::Oscillate];
    pub fn name(self) -> &'static str {
        match self {
            Infinity::Constant => "constant",
            Infinity::Linear => "linear",
            Infinity::Cycle => "cycle",
            Infinity::CycleRelative => "cycleRelative",
            Infinity::Oscillate => "oscillate",
        }
    }
    pub fn parse(s: &str) -> Option<Infinity> {
        Infinity::ALL.into_iter().find(|t| t.name() == s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub time: f64,
    pub value: f64,
    pub in_tangent: Tangent,
    pub out_tangent: Tangent,
    /// Slopes used when the tangent type is `Fixed`.
    #[serde(default)]
    pub in_slope: f64,
    #[serde(default)]
    pub out_slope: f64,
}

impl Key {
    pub fn new(time: f64, value: f64) -> Self {
        Self { time, value, in_tangent: Tangent::Auto, out_tangent: Tangent::Auto, in_slope: 0.0, out_slope: 0.0 }
    }
    pub fn with_tangents(mut self, t: Tangent) -> Self {
        self.in_tangent = t;
        self.out_tangent = t;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnimCurve {
    pub keys: Vec<Key>,
    #[serde(default)]
    pub pre_infinity: Infinity,
    #[serde(default)]
    pub post_infinity: Infinity,
}

const EPS: f64 = 1e-6;

impl AnimCurve {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_keys(keys: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let mut c = Self::new();
        for (t, v) in keys {
            c.set_key(t, v);
        }
        c
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Insert or replace a key, keeping the list sorted. Returns its index.
    pub fn set_key(&mut self, time: f64, value: f64) -> usize {
        match self.find(time) {
            Ok(i) => {
                self.keys[i].value = value;
                i
            }
            Err(i) => {
                // inherit tangent types from the neighbor (Maya uses prefs default)
                let mut k = Key::new(time, value);
                if let Some(n) = self.keys.get(i.saturating_sub(1)) {
                    if n.out_tangent == Tangent::Stepped {
                        k.in_tangent = Tangent::Stepped;
                        k.out_tangent = Tangent::Stepped;
                    }
                }
                self.keys.insert(i, k);
                i
            }
        }
    }

    pub fn insert(&mut self, key: Key) -> usize {
        match self.find(key.time) {
            Ok(i) => {
                self.keys[i] = key;
                i
            }
            Err(i) => {
                self.keys.insert(i, key);
                i
            }
        }
    }

    fn find(&self, time: f64) -> Result<usize, usize> {
        for (i, k) in self.keys.iter().enumerate() {
            if (k.time - time).abs() < EPS {
                return Ok(i);
            }
            if k.time > time {
                return Err(i);
            }
        }
        Err(self.keys.len())
    }

    pub fn key_at(&self, time: f64) -> Option<usize> {
        self.find(time).ok()
    }

    pub fn remove_key_at(&mut self, time: f64) -> bool {
        if let Ok(i) = self.find(time) {
            self.keys.remove(i);
            true
        } else {
            false
        }
    }

    /// Remove keys whose time lies in [t0, t1].
    pub fn remove_range(&mut self, t0: f64, t1: f64) -> usize {
        let before = self.keys.len();
        self.keys.retain(|k| k.time < t0 - EPS || k.time > t1 + EPS);
        before - self.keys.len()
    }

    pub fn sort(&mut self) {
        self.keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        self.keys.dedup_by(|a, b| (a.time - b.time).abs() < EPS);
    }

    pub fn set_tangents(&mut self, idx: usize, t: Tangent) {
        if let Some(k) = self.keys.get_mut(idx) {
            k.in_tangent = t;
            k.out_tangent = t;
        }
    }

    pub fn first_time(&self) -> Option<f64> {
        self.keys.first().map(|k| k.time)
    }
    pub fn last_time(&self) -> Option<f64> {
        self.keys.last().map(|k| k.time)
    }

    pub fn next_key_time(&self, t: f64) -> Option<f64> {
        self.keys.iter().map(|k| k.time).find(|&kt| kt > t + EPS)
    }
    pub fn prev_key_time(&self, t: f64) -> Option<f64> {
        self.keys.iter().rev().map(|k| k.time).find(|&kt| kt < t - EPS)
    }

    /// Slope (value per frame) of the incoming / outgoing tangent at key i.
    pub fn slopes(&self, i: usize) -> (f64, f64) {
        let k = &self.keys[i];
        let prev = i.checked_sub(1).map(|j| self.keys[j]);
        let next = self.keys.get(i + 1).copied();
        let left = prev.map(|p| (k.value - p.value) / (k.time - p.time).max(EPS));
        let right = next.map(|n| (n.value - k.value) / (n.time - k.time).max(EPS));
        let spline = match (prev, next) {
            (Some(p), Some(n)) => (n.value - p.value) / (n.time - p.time).max(EPS),
            (Some(_), None) => left.unwrap(),
            (None, Some(_)) => right.unwrap(),
            (None, None) => 0.0,
        };
        let clamped = || -> f64 {
            match (left, right) {
                (Some(l), Some(r)) => {
                    if l * r <= 0.0 {
                        0.0
                    } else {
                        // Fritsch–Carlson limiter: no overshoot.
                        let s = spline;
                        let lim = 3.0 * l.abs().min(r.abs());
                        s.signum() * s.abs().min(lim)
                    }
                }
                _ => 0.0,
            }
        };
        let eval = |t: Tangent, fixed: f64, side_linear: Option<f64>| match t {
            Tangent::Auto => clamped(),
            Tangent::Clamped => {
                // Clamped: spline unless the neighbors are nearly equal.
                match (prev, next) {
                    (Some(p), _) if (p.value - k.value).abs() < 1e-9 => 0.0,
                    (_, Some(n)) if (n.value - k.value).abs() < 1e-9 => 0.0,
                    _ => spline,
                }
            }
            Tangent::Spline => spline,
            Tangent::Linear => side_linear.unwrap_or(0.0),
            Tangent::Flat | Tangent::Stepped => 0.0,
            Tangent::Fixed => fixed,
        };
        (eval(k.in_tangent, k.in_slope, left), eval(k.out_tangent, k.out_slope, right))
    }

    /// Evaluate within the keyed range (no infinity handling).
    fn eval_inner(&self, t: f64) -> f64 {
        let n = self.keys.len();
        if t <= self.keys[0].time {
            return self.keys[0].value;
        }
        if t >= self.keys[n - 1].time {
            return self.keys[n - 1].value;
        }
        let i = self.keys.partition_point(|k| k.time <= t) - 1;
        let k0 = self.keys[i];
        let k1 = self.keys[i + 1];
        let dt = k1.time - k0.time;
        if k0.out_tangent == Tangent::Stepped {
            return k0.value;
        }
        let s = (t - k0.time) / dt;
        if k0.out_tangent == Tangent::Linear && k1.in_tangent == Tangent::Linear {
            return k0.value + (k1.value - k0.value) * s;
        }
        let m0 = self.slopes(i).1 * dt;
        let m1 = self.slopes(i + 1).0 * dt;
        let s2 = s * s;
        let s3 = s2 * s;
        let h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let h10 = s3 - 2.0 * s2 + s;
        let h01 = -2.0 * s3 + 3.0 * s2;
        let h11 = s3 - s2;
        h00 * k0.value + h10 * m0 + h01 * k1.value + h11 * m1
    }

    pub fn evaluate(&self, t: f64) -> f64 {
        let n = self.keys.len();
        if n == 0 {
            return 0.0;
        }
        if n == 1 {
            return self.keys[0].value;
        }
        let first = self.keys[0];
        let last = self.keys[n - 1];
        let span = last.time - first.time;
        if t < first.time {
            return self.extrapolate(t, self.pre_infinity, true, first, last, span);
        }
        if t > last.time {
            return self.extrapolate(t, self.post_infinity, false, first, last, span);
        }
        self.eval_inner(t)
    }

    fn extrapolate(&self, t: f64, inf: Infinity, pre: bool, first: Key, last: Key, span: f64) -> f64 {
        if span <= EPS {
            return if pre { first.value } else { last.value };
        }
        match inf {
            Infinity::Constant => {
                if pre { first.value } else { last.value }
            }
            Infinity::Linear => {
                if pre {
                    // the first key's outgoing slope continues backwards
                    let s = self.slopes(0).1;
                    first.value - s * (first.time - t)
                } else {
                    let s = self.slopes(self.keys.len() - 1).0;
                    last.value + s * (t - last.time)
                }
            }
            Infinity::Cycle | Infinity::CycleRelative | Infinity::Oscillate => {
                let rel = t - first.time;
                let cycles = (rel / span).floor();
                let mut local = rel - cycles * span;
                if inf == Infinity::Oscillate && (cycles as i64).rem_euclid(2) == 1 {
                    local = span - local;
                }
                let v = self.eval_inner(first.time + local);
                if inf == Infinity::CycleRelative {
                    v + cycles * (last.value - first.value)
                } else {
                    v
                }
            }
        }
    }

    /// Move keys in [t0,t1] by `dt` frames and `dv` value units.
    pub fn offset_keys(&mut self, t0: f64, t1: f64, dt: f64, dv: f64) {
        for k in &mut self.keys {
            if k.time >= t0 - EPS && k.time <= t1 + EPS {
                k.time += dt;
                k.value += dv;
            }
        }
        self.sort();
    }

    /// Scale key times about a pivot time.
    pub fn scale_time(&mut self, pivot: f64, factor: f64) {
        for k in &mut self.keys {
            k.time = pivot + (k.time - pivot) * factor;
        }
        self.sort();
    }

    /// Resample into a key on every `step` frames between t0 and t1.
    pub fn bake(&mut self, t0: f64, t1: f64, step: f64) {
        let step = step.max(EPS);
        let mut baked = Vec::new();
        let mut t = t0;
        while t <= t1 + EPS {
            baked.push(Key::new(t, self.evaluate(t)).with_tangents(Tangent::Linear));
            t += step;
        }
        self.keys.retain(|k| k.time < t0 - EPS || k.time > t1 + EPS);
        self.keys.extend(baked);
        self.sort();
    }

    /// Remove keys that can be reproduced within `tol` by their neighbors.
    pub fn simplify(&mut self, tol: f64) -> usize {
        let mut removed = 0;
        let mut i = 1;
        while i + 1 < self.keys.len() {
            let mut trial = self.clone();
            let k = trial.keys.remove(i);
            let mid_ok = {
                let t0 = self.keys[i - 1].time;
                let t1 = self.keys[i + 1].time;
                (0..=8).all(|s| {
                    let t = t0 + (t1 - t0) * s as f64 / 8.0;
                    (trial.evaluate(t) - self.evaluate(t)).abs() <= tol
                })
            } && (trial.evaluate(k.time) - k.value).abs() <= tol;
            if mid_ok {
                *self = trial;
                removed += 1;
            } else {
                i += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_and_step() {
        let mut c = AnimCurve::from_keys([(0.0, 0.0), (10.0, 10.0)]);
        for k in &mut c.keys {
            k.in_tangent = Tangent::Linear;
            k.out_tangent = Tangent::Linear;
        }
        assert!((c.evaluate(5.0) - 5.0).abs() < 1e-9);
        c.keys[0].out_tangent = Tangent::Stepped;
        assert_eq!(c.evaluate(9.9), 0.0);
        assert_eq!(c.evaluate(10.0), 10.0);
    }

    #[test]
    fn auto_no_overshoot() {
        let c = AnimCurve::from_keys([(0.0, 0.0), (10.0, 10.0), (20.0, 10.0), (30.0, 0.0)]);
        for i in 0..=300 {
            let v = c.evaluate(i as f64 * 0.1);
            assert!((-1e-9..=10.0 + 1e-9).contains(&v), "overshoot {v} at {}", i as f64 * 0.1);
        }
        // passes through keys
        assert!((c.evaluate(10.0) - 10.0).abs() < 1e-9);
        // smooth ease: midpoint of first segment is half way by symmetry
        assert!((c.evaluate(5.0) - 5.0).abs() < 2.0);
    }

    #[test]
    fn infinity_modes() {
        let mut c = AnimCurve::from_keys([(0.0, 0.0), (10.0, 10.0)]);
        c.set_tangents(0, Tangent::Linear);
        c.set_tangents(1, Tangent::Linear);
        c.post_infinity = Infinity::Cycle;
        assert!((c.evaluate(15.0) - 5.0).abs() < 1e-9);
        c.post_infinity = Infinity::CycleRelative;
        assert!((c.evaluate(15.0) - 15.0).abs() < 1e-9);
        c.post_infinity = Infinity::Oscillate;
        assert!((c.evaluate(12.0) - 8.0).abs() < 1e-9);
        c.post_infinity = Infinity::Linear;
        assert!((c.evaluate(20.0) - 20.0).abs() < 1e-9);
        c.pre_infinity = Infinity::Constant;
        assert_eq!(c.evaluate(-5.0), 0.0);
    }

    #[test]
    fn set_replace_remove() {
        let mut c = AnimCurve::new();
        c.set_key(5.0, 1.0);
        c.set_key(1.0, 2.0);
        c.set_key(5.0, 3.0);
        assert_eq!(c.len(), 2);
        assert_eq!(c.keys[0].time, 1.0);
        assert_eq!(c.keys[1].value, 3.0);
        assert_eq!(c.next_key_time(1.0), Some(5.0));
        assert_eq!(c.prev_key_time(5.0), Some(1.0));
        assert!(c.remove_key_at(1.0));
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn bake_and_simplify() {
        let mut c = AnimCurve::from_keys([(0.0, 0.0), (10.0, 10.0)]);
        c.set_tangents(0, Tangent::Linear);
        c.set_tangents(1, Tangent::Linear);
        c.bake(0.0, 10.0, 1.0);
        assert_eq!(c.len(), 11);
        let removed = c.simplify(1e-6);
        assert_eq!(removed, 9);
        assert!((c.evaluate(3.0) - 3.0).abs() < 1e-9);
    }
}
