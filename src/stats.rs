//! The two order statistics prot-scriber computes over word scores. Both are exact
//! reimplementations of the `statrs` functions they replace, because their results reach the
//! output: `quantile` is the value word scores are centered on, so a different definition of
//! "quantile" changes which words end up in a human readable description.

/// The arithmetic mean, computed incrementally as `statrs`'s `Distribution::mean` does it, i.e.
/// Welford's method rather than a sum divided by the count. The distinction is not cosmetic:
/// `sum / n` and `m += (x - m) / i` differ in the last bits, and prot-scriber's word scores are
/// centered on this value, so a word sitting on the zero crossing can move.
///
/// Returns `f64::NAN` for empty input, as `statrs` does.
///
/// # Arguments
///
/// * `values` - The values to average.
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut i = 0.0;
    let mut mean = 0.0;
    for x in values {
        i += 1.0;
        mean += (*x - mean) / i;
    }
    mean
}

/// The `tau`-th quantile, estimated as `statrs`'s `OrderStatistics::quantile` estimates it: the
/// Hyndman-Fan type 8 definition, `h = (n + 1/3) * tau + 1/3`, interpolating linearly between the
/// two order statistics that bracket `h`. Note that this is one of nine textbook definitions of
/// "quantile" and the others give different answers, which is why it is spelled out here.
///
/// Returns `f64::NAN` if `values` is empty or `tau` lies outside `[0, 1]`, as `statrs` does.
///
/// `statrs` selects the two bracketing order statistics with quickselect; sorting and indexing
/// returns the very same elements, since both compute exact order statistics, and the arithmetic
/// that follows is identical. The input is expected to be free of `NaN` - it holds word scores
/// derived from word frequencies - and is reordered in place either way.
///
/// # Arguments
///
/// * `values` - The values to take the quantile of. Reordered in place.
/// * `tau` - The quantile to estimate, between zero and one (both inclusive).
pub fn quantile(values: &mut [f64], tau: f64) -> f64 {
    if !(0.0..=1.0).contains(&tau) || values.is_empty() {
        return f64::NAN;
    }

    let n = values.len();
    let h = (n as f64 + 1.0 / 3.0) * tau + 1.0 / 3.0;
    let hf = h as i64;

    values.sort_by(f64::total_cmp);

    if hf <= 0 || tau == 0.0 {
        return values[0];
    }
    if hf >= n as i64 || ulps_eq_one(tau) {
        return values[n - 1];
    }

    let a = values[hf as usize - 1];
    let b = values[hf as usize];
    a + (h - hf as f64) * (b - a)
}

/// Whether `tau` equals one to within four units in the last place, the comparison `statrs` makes
/// with `approx`'s `ulps_eq!` at its own default tolerances. Reproduced here rather than depending
/// on `approx` at run time, since `approx` is only needed by the tests.
///
/// # Arguments
///
/// * `tau` - The value to compare against one.
fn ulps_eq_one(tau: f64) -> bool {
    if (tau - 1.0).abs() <= f64::EPSILON {
        return true;
    }
    if tau.signum() != 1f64.signum() {
        return false;
    }
    let (bits_tau, bits_one) = (tau.to_bits(), 1f64.to_bits());
    if bits_tau <= bits_one {
        bits_one - bits_tau <= 4
    } else {
        bits_tau - bits_one <= 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The expected values below were produced by `statrs` 0.15.0 itself, before it was removed as
    /// a dependency, and are compared bit for bit rather than approximately: they pin the exact
    /// quantile definition, not merely a nearby number. The implementation was additionally
    /// checked against `statrs` over 6661 randomised quantile cases and 512 means, all identical
    /// to the last bit.
    const INPUT: [f64; 13] = [
        -3.5, 0.0, 1.25, 7.0, 2.5, -0.75, 4.0, 1.25, 9.5, -6.25, 0.5, 3.0, 2.0,
    ];

    #[test]
    fn quantile_is_the_hyndman_fan_type_8_estimate() {
        let expected: [(f64, u64); 10] = [
            (0.0,           0xc019000000000000), // -6.25            (the minimum)
            (0.05,          0xc019000000000000), // -6.25
            (1.0 / 3.0,     0x3fd8e38e38e38e38), //  0.38888888888888884
            (0.4,           0x3ff0000000000001), //  1.0000000000000002
            (0.5,           0x3ff4000000000000), //  1.25
            (0.75,          0x400aaaaaaaaaaaac), //  3.333333333333334
            (0.8,           0x4010000000000006), //  4.000000000000005
            (0.95,          0x4023000000000000), //  9.5
            (1.0,           0x4023000000000000), //  9.5               (the maximum)
            (0.123456789,   0xc00c73e2860e3e23), // -3.5565844033333334
        ];
        for (tau, bits) in expected {
            let mut values = INPUT.to_vec();
            assert_eq!(
                quantile(&mut values, tau).to_bits(),
                bits,
                "quantile at tau={:?} is {:?}, expected {:?}",
                tau,
                quantile(&mut INPUT.to_vec(), tau),
                f64::from_bits(bits)
            );
        }
    }

    #[test]
    fn quantile_of_degenerate_input() {
        // empty input, and a tau outside [0, 1], are NaN rather than a panic
        assert!(quantile(&mut [], 0.5).is_nan());
        assert!(quantile(&mut [1.0, 2.0], -0.1).is_nan());
        assert!(quantile(&mut [1.0, 2.0], 1.1).is_nan());
        // a single value is every one of its own quantiles
        assert_eq!(quantile(&mut [42.0], 0.0), 42.0);
        assert_eq!(quantile(&mut [42.0], 0.5), 42.0);
        assert_eq!(quantile(&mut [42.0], 1.0), 42.0);
        // as is a constant sample
        assert_eq!(quantile(&mut [2.0; 7], 0.5), 2.0);
    }

    #[test]
    fn quantile_does_not_depend_on_the_order_of_its_input() {
        let mut ascending = INPUT.to_vec();
        ascending.sort_by(f64::total_cmp);
        let mut descending = ascending.clone();
        descending.reverse();
        for tau in [0.0, 0.25, 1.0 / 3.0, 0.5, 0.75, 1.0] {
            let from_original = quantile(&mut INPUT.to_vec(), tau);
            assert_eq!(quantile(&mut ascending.clone(), tau), from_original);
            assert_eq!(quantile(&mut descending.clone(), tau), from_original);
        }
    }

    #[test]
    fn mean_is_welfords_method_and_not_a_sum_divided_by_the_count() {
        // Six copies of 5.6: Welford returns 5.6 exactly, `sum / n` returns 5.6000000000000005.
        // The two differ in the last bit, and this value is what word scores are centered on, so
        // replacing the one with the other is not a simplification -- it moves annotations.
        let constant = [5.6f64; 6];
        let naive = constant.iter().sum::<f64>() / constant.len() as f64;
        assert_eq!(mean(&constant).to_bits(), 5.6f64.to_bits());
        assert_ne!(mean(&constant).to_bits(), naive.to_bits());

        assert_eq!(mean(&INPUT).to_bits(), 0x3ff93b13b13b13b2); // 1.576923076923077
    }

    #[test]
    fn mean_of_empty_input_is_nan() {
        assert!(mean(&[]).is_nan());
    }
}
