//! Vector math over columns: what math code spaces compile to, and handy in Rust ones.
//! Missing values are `NaN`; windows that are not full yet give `NaN`.

/// `n` copies of `v`.
pub fn k(v: f64, n: usize) -> Vec<f64> {
    vec![v; n]
}

/// Element by element (`a` and `b` have the same length).
pub fn zip(a: &[f64], b: &[f64], f: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    a.iter().zip(b).map(|(x, y)| f(*x, *y)).collect()
}

pub fn map(a: &[f64], f: impl Fn(f64) -> f64) -> Vec<f64> {
    a.iter().map(|x| f(*x)).collect()
}

/// `if(c, a, b)`: `a` where `c` is true (not 0, not NaN), else `b`.
pub fn choose(c: &[f64], a: &[f64], b: &[f64]) -> Vec<f64> {
    c.iter().enumerate().map(|(i, x)| if truthy(*x) { a[i] } else { b[i] }).collect()
}

pub fn truthy(x: f64) -> bool {
    x != 0.0 && !x.is_nan()
}

pub fn flag(b: bool) -> f64 {
    if b {
        1.0
    } else {
        0.0
    }
}

/// The first value of a window/period argument, as a count (at least 1).
pub fn count_arg(w: &[f64]) -> usize {
    w.first().copied().filter(|x| x.is_finite() && *x >= 1.0).unwrap_or(1.0) as usize
}

fn rolling(a: &[f64], n: usize, f: impl Fn(&[f64]) -> f64) -> Vec<f64> {
    (0..a.len()).map(|i| if i + 1 < n { f64::NAN } else { f(&a[i + 1 - n..=i]) }).collect()
}

fn mean_of(w: &[f64]) -> f64 {
    w.iter().sum::<f64>() / w.len() as f64
}

fn std_of(w: &[f64]) -> f64 {
    if w.len() < 2 {
        return f64::NAN;
    }
    let m = mean_of(w);
    (w.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (w.len() - 1) as f64).sqrt()
}

/// Simple moving average over `n` values.
pub fn sma(a: &[f64], n: &[f64]) -> Vec<f64> {
    rolling(a, count_arg(n), mean_of)
}

/// Exponential moving average with span `n` (alpha = 2 / (n + 1)), seeded with the first value.
pub fn ema(a: &[f64], n: &[f64]) -> Vec<f64> {
    let alpha = 2.0 / (count_arg(n) as f64 + 1.0);
    let mut out = Vec::with_capacity(a.len());
    let mut prev = f64::NAN;
    for &x in a {
        prev = if prev.is_nan() {
            x
        } else if x.is_nan() {
            prev
        } else {
            alpha * x + (1.0 - alpha) * prev
        };
        out.push(prev);
    }
    out
}

pub fn rolling_sum(a: &[f64], n: &[f64]) -> Vec<f64> {
    rolling(a, count_arg(n), |w| w.iter().sum())
}
pub fn rolling_min(a: &[f64], n: &[f64]) -> Vec<f64> {
    rolling(a, count_arg(n), |w| w.iter().copied().fold(f64::INFINITY, f64::min))
}
pub fn rolling_max(a: &[f64], n: &[f64]) -> Vec<f64> {
    rolling(a, count_arg(n), |w| w.iter().copied().fold(f64::NEG_INFINITY, f64::max))
}
pub fn rolling_std(a: &[f64], n: &[f64]) -> Vec<f64> {
    rolling(a, count_arg(n), std_of)
}

/// The value `n` rows earlier.
pub fn lag(a: &[f64], n: &[f64]) -> Vec<f64> {
    let n = count_arg(n);
    (0..a.len()).map(|i| if i >= n { a[i - n] } else { f64::NAN }).collect()
}

/// The value `n` rows later.
pub fn lead(a: &[f64], n: &[f64]) -> Vec<f64> {
    let n = count_arg(n);
    (0..a.len()).map(|i| a.get(i + n).copied().unwrap_or(f64::NAN)).collect()
}

/// Change from the previous row.
pub fn diff(a: &[f64]) -> Vec<f64> {
    (0..a.len()).map(|i| if i == 0 { f64::NAN } else { a[i] - a[i - 1] }).collect()
}

/// Change from the previous row in percent.
pub fn pct_change(a: &[f64]) -> Vec<f64> {
    (0..a.len()).map(|i| if i == 0 { f64::NAN } else { (a[i] - a[i - 1]) / a[i - 1] * 100.0 }).collect()
}

/// Running total (NaN counts as 0).
pub fn cumsum(a: &[f64]) -> Vec<f64> {
    let mut s = 0.0;
    a.iter()
        .map(|x| {
            if !x.is_nan() {
                s += x;
            }
            s
        })
        .collect()
}

fn finite(a: &[f64]) -> Vec<f64> {
    a.iter().copied().filter(|x| !x.is_nan()).collect()
}

/// Aggregates: one value, repeated for every row.
pub fn sum(a: &[f64]) -> Vec<f64> {
    k(finite(a).iter().sum(), a.len())
}
pub fn mean(a: &[f64]) -> Vec<f64> {
    let f = finite(a);
    k(if f.is_empty() { f64::NAN } else { mean_of(&f) }, a.len())
}
pub fn min(a: &[f64]) -> Vec<f64> {
    k(finite(a).into_iter().fold(f64::NAN, f64::min), a.len())
}
pub fn max(a: &[f64]) -> Vec<f64> {
    k(finite(a).into_iter().fold(f64::NAN, f64::max), a.len())
}
pub fn std(a: &[f64]) -> Vec<f64> {
    k(std_of(&finite(a)), a.len())
}
pub fn count(a: &[f64]) -> Vec<f64> {
    k(finite(a).len() as f64, a.len())
}
pub fn first(a: &[f64]) -> Vec<f64> {
    k(a.first().copied().unwrap_or(f64::NAN), a.len())
}
pub fn last(a: &[f64]) -> Vec<f64> {
    k(a.last().copied().unwrap_or(f64::NAN), a.len())
}

/// Standard score against the whole column.
pub fn zscore(a: &[f64]) -> Vec<f64> {
    let f = finite(a);
    let (m, s) = (mean_of(&f), std_of(&f));
    map(a, |x| (x - m) / s)
}

/// Row number from 0.
pub fn row(n: usize) -> Vec<f64> {
    (0..n).map(|i| i as f64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_and_aggregates() {
        let a = [1.0, 2.0, 3.0, 4.0];
        let s = sma(&a, &[2.0]);
        assert!(s[0].is_nan());
        assert_eq!(&s[1..], &[1.5, 2.5, 3.5]);
        assert_eq!(lag(&a, &[1.0])[1], 1.0);
        assert_eq!(cumsum(&a), vec![1.0, 3.0, 6.0, 10.0]);
        assert_eq!(sum(&a), vec![10.0; 4]);
        assert_eq!(ema(&a, &[1.0]), a.to_vec());
        assert_eq!(choose(&[1.0, 0.0], &[5.0, 5.0], &[7.0, 7.0]), vec![5.0, 7.0]);
        assert_eq!(rolling_max(&a, &[3.0])[3], 4.0);
        assert_eq!(pct_change(&a)[1], 100.0);
    }
}
