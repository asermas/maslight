//! Mapping a photograph onto the screen it contains.
//!
//! Nobody photographs a monitor perfectly square on, so the screen arrives as
//! a general quadrilateral. A homography is the transform that takes those
//! four corners back to a unit square, and it is the difference between LED
//! positions that are roughly right and LED positions that are right.
//!
//! Solved by direct linear transformation: four point correspondences give
//! eight equations in the eight unknowns of a 3x3 matrix whose last entry is
//! fixed at one. Gaussian elimination on an 8x8 system is a few dozen lines
//! and needs no linear algebra dependency.

/// A plane to plane transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homography {
    m: [f32; 9],
}

impl Default for Homography {
    fn default() -> Self {
        Self::identity()
    }
}

impl Homography {
    pub fn identity() -> Self {
        Self {
            m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        }
    }

    /// The transform taking the four screen corners in the photograph to the
    /// unit square, in the order top left, top right, bottom right, bottom
    /// left.
    ///
    /// Returns `None` when the corners are degenerate, which in practice means
    /// three of them are in a line.
    pub fn from_screen_corners(corners: [(f32, f32); 4]) -> Option<Self> {
        let target = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        Self::from_correspondences(corners, target)
    }

    /// The transform taking `from` to `to`.
    pub fn from_correspondences(from: [(f32, f32); 4], to: [(f32, f32); 4]) -> Option<Self> {
        // Each correspondence contributes two rows:
        //   x h0 + y h1 + h2 - u x h6 - u y h7 = u
        //   x h3 + y h4 + h5 - v x h6 - v y h7 = v
        let mut a = [[0.0f64; 9]; 8];
        for i in 0..4 {
            let (x, y) = (from[i].0 as f64, from[i].1 as f64);
            let (u, v) = (to[i].0 as f64, to[i].1 as f64);

            let row = &mut a[i * 2];
            row[0] = x;
            row[1] = y;
            row[2] = 1.0;
            row[6] = -u * x;
            row[7] = -u * y;
            row[8] = u;

            let row = &mut a[i * 2 + 1];
            row[3] = x;
            row[4] = y;
            row[5] = 1.0;
            row[6] = -v * x;
            row[7] = -v * y;
            row[8] = v;
        }

        let solution = solve(&mut a)?;
        Some(Self {
            m: [
                solution[0] as f32,
                solution[1] as f32,
                solution[2] as f32,
                solution[3] as f32,
                solution[4] as f32,
                solution[5] as f32,
                solution[6] as f32,
                solution[7] as f32,
                1.0,
            ],
        })
    }

    /// Map a point.
    pub fn map(&self, x: f32, y: f32) -> (f32, f32) {
        let m = &self.m;
        let w = m[6] * x + m[7] * y + m[8];
        // A vanishing denominator means the point is on the horizon of the
        // transform. Returning it unchanged is wrong but finite, and the
        // caller clamps anyway.
        if w.abs() < 1e-9 {
            return (x, y);
        }
        (
            (m[0] * x + m[1] * y + m[2]) / w,
            (m[3] * x + m[4] * y + m[5]) / w,
        )
    }
}

/// Gaussian elimination with partial pivoting on an 8x9 augmented matrix.
fn solve(a: &mut [[f64; 9]; 8]) -> Option<[f64; 8]> {
    for col in 0..8 {
        // Pivot on the largest remaining magnitude, or the system is unstable
        // for corners that are nearly in a line.
        let mut pivot = col;
        for row in (col + 1)..8 {
            if a[row][col].abs() > a[pivot][col].abs() {
                pivot = row;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);

        let divisor = a[col][col];
        for value in a[col][col..].iter_mut() {
            *value /= divisor;
        }
        let pivot_row = a[col];
        for (row, values) in a.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let factor = values[col];
            if factor == 0.0 {
                continue;
            }
            for (k, value) in values.iter_mut().enumerate().skip(col) {
                *value -= factor * pivot_row[k];
            }
        }
    }

    let mut out = [0.0f64; 8];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = a[i][8];
        if !slot.is_finite() {
            return None;
        }
    }
    Some(out)
}
