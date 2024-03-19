/// Represents width and height in a tuple.
type Dims = (u32, u32);

/// Represents the possible resize strategies.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Resize {
    /// When resizing with `Resize::Exact`, each frame will be resized to the exact width and height
    /// given, without taking into account aspect ratio.
    Exact(u32, u32),
    /// When resizing with `Resize::Fit`, each frame will be resized to the biggest width and height
    /// possible within the given dimensions, without changing the aspect ratio.
    Fit(u32, u32),
    /// When resizing with `Resize::FitEven`, each frame will be resized to the biggest even width
    /// and height possible within the given dimensions, maintaining aspect ratio. Resizing using
    /// this method can fail if there exist no dimensions that fit these constraints.
    ///
    /// Note that this resizing method is especially useful since some encoders only accept frames
    /// with dimensions that are divisible by 2.
    FitEven(u32, u32),
}

impl Resize {
    /// Compute the dimensions after resizing depending on the resize strategy.
    ///
    /// # Arguments
    ///
    /// * `dims` - Input dimensions (width and height).
    ///
    /// # Return value
    ///
    /// Tuple of width and height with dimensions after resizing.
    pub fn compute_for(self, dims: Dims) -> Option<Dims> {
        match self {
            Resize::Exact(w, h) => Some((w, h)),
            Resize::Fit(w, h) => calculate_fit_dims(dims, (w, h)),
            Resize::FitEven(w, h) => calculate_fit_dims_even(dims, (w, h)),
        }
    }
}

/// Calculates the maximum image dimensions `w` and `h` that fit inside `w_max` and `h_max`
/// retaining the original aspect ratio.
///
/// # Arguments
///
/// * `dims` - Original dimensions: width and height.
/// * `fit_dims` - Dimensions to fit in: width and height.
///
/// # Return value
///
/// The fitted dimensions if they exist and are positive and more than zero.
fn calculate_fit_dims(dims: (u32, u32), fit_dims: (u32, u32)) -> Option<(u32, u32)> {
    let (w, h) = dims;
    let (w_max, h_max) = fit_dims;
    if w_max >= w && h_max >= h {
        Some((w, h))
    } else {
        let wf = w_max as f32 / w as f32;
        let hf = h_max as f32 / h as f32;
        let f = wf.min(hf);
        let (w_out, h_out) = ((w as f32 * f) as u32, (h as f32 * f) as u32);
        if (w_out > 0) && (h_out > 0) {
            Some((w_out, h_out))
        } else {
            None
        }
    }
}

/// Calculates the maximum image dimensions `w` and `h` that fit inside `w_max` and `h_max`
/// retaining the original aspect ratio, where both the width and height must be divisble by two.
///
/// Note that this method will even reduce the dimensions to even width and height if they already
/// fit in `fit_dims`.
///
/// # Arguments
///
/// * `dims` - Original dimensions: width and height.
/// * `fit_dims` - Dimensions to fit in: width and height.
///
/// # Return value
///
/// The fitted dimensions if they exist and are positive and more than zero.
fn calculate_fit_dims_even(dims: (u32, u32), fit_dims: (u32, u32)) -> Option<(u32, u32)> {
    let (w, h) = dims;
    let (mut w_max, mut h_max) = fit_dims;
    while w_max > 0 && h_max > 0 {
        let wf = w_max as f32 / w as f32;
        let hf = h_max as f32 / h as f32;
        let f = wf.min(hf).min(1.0);
        let out_w = (w as f32 * f).round() as u32;
        let out_h = (h as f32 * f).round() as u32;
        if (out_w > 0) && (out_h > 0) {
            if (out_w % 2 == 0) && (out_h % 2 == 0) {
                return Some((out_w, out_h));
            } else if wf < hf {
                w_max -= 1;
            } else {
                h_max -= 1;
            }
        } else {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const TESTING_DIM_CANDIDATES: [u32; 8] = [0, 1, 2, 3, 8, 111, 256, 1000];

    #[test]
    fn calculate_fit_dims_works() {
        let testset = generate_testset();
        for ((w, h), (fit_w, fit_h)) in testset {
            let out = calculate_fit_dims((w, h), (fit_w, fit_h));
            if let Some((out_w, out_h)) = out {
                let input_dim_zero = w == 0 || h == 0 || fit_w == 0 || fit_h == 0;
                let output_dim_zero = out_w == 0 || out_h == 0;
                assert!(
                    (input_dim_zero && output_dim_zero) || (!input_dim_zero && !output_dim_zero),
                    "computed dims are never zero unless the inputs dims were",
                );
                assert!(
                    (out_w <= fit_w) && (out_h <= fit_h),
                    "computed dims fit inside provided dims",
                );
            }
        }
    }

    #[test]
    fn calculate_fit_dims_even_works() {
        let testset = generate_testset();
        for ((w, h), (fit_w, fit_h)) in testset {
            let out = calculate_fit_dims_even((w, h), (fit_w, fit_h));
            if let Some((out_w, out_h)) = out {
                let input_dim_zero = w == 0 || h == 0 || fit_w == 0 || fit_h == 0;
                let output_dim_zero = out_w == 0 || out_h == 0;
                assert!(
                    (input_dim_zero && output_dim_zero) || (!input_dim_zero && !output_dim_zero),
                    "computed dims are never zero unless the inputs dims were",
                );
                assert!(
                    (out_w % 2 == 0) && (out_h % 2 == 0),
                    "computed dims are even",
                );
                assert!(
                    (out_w <= fit_w) && (out_h <= fit_h),
                    "computed dims fit inside provided dims",
                );
            }
        }
    }

    fn generate_testset() -> Vec<((u32, u32), (u32, u32))> {
        let testing_dims = generate_testing_dims();
        testing_dims
            .iter()
            .flat_map(|dims| testing_dims.iter().map(|fit_dims| (*dims, *fit_dims)))
            .collect()
    }

    fn generate_testing_dims() -> Vec<(u32, u32)> {
        TESTING_DIM_CANDIDATES
            .iter()
            .flat_map(|a| TESTING_DIM_CANDIDATES.iter().map(|b| (*a, *b)))
            .collect()
    }
}
