#![allow(dead_code)]
use crate::poly::field::JoltField;
use crate::utils::gaussian_elimination::gaussian_elimination;
use crate::utils::transcript::{AppendToTranscript, ProofTranscript};
use ark_serialize::*;

// ax^2 + bx + c stored as vec![c,b,a]
// ax^3 + bx^2 + cx + d stored as vec![d,c,b,a]
#[derive(Debug, Clone, PartialEq)]
pub struct UniPoly<F> {
    coeffs: Vec<F>,
}

// ax^2 + bx + c stored as vec![c,a]
// ax^3 + bx^2 + cx + d stored as vec![d,b,a]
#[derive(CanonicalSerialize, CanonicalDeserialize, Debug)]
pub struct CompressedUniPoly<F: JoltField> {
    coeffs_except_linear_term: Vec<F>,
}

impl<F: JoltField> UniPoly<F> {
    #[allow(dead_code)]
    pub fn from_coeff(coeffs: Vec<F>) -> Self {
        UniPoly { coeffs }
    }

    pub fn from_evals(evals: &[F]) -> Self {
        UniPoly {
            coeffs: Self::vandermonde_interpolation(evals),
        }
    }

    fn vandermonde_interpolation(evals: &[F]) -> Vec<F> {
        let n = evals.len();
        let xs: Vec<F> = (0..n).map(|x| F::from_u64(x as u64).unwrap()).collect();

        let mut vandermonde: Vec<Vec<F>> = Vec::with_capacity(n);
        for i in 0..n {
            let mut row = Vec::with_capacity(n);
            let x = xs[i];
            row.push(F::one());
            row.push(x);
            for j in 2..n {
                row.push(row[j - 1] * x);
            }
            row.push(evals[i]);
            vandermonde.push(row);
        }

        gaussian_elimination(&mut vandermonde)
    }

    /// Divide self by another polynomial, and returns the
    /// quotient and remainder.
    pub fn divide_with_q_and_r(&self, divisor: &Self) -> Option<(Self, Self)> {
        if self.is_zero() {
            Some((Self::zero(), Self::zero()))
        } else if divisor.is_zero() {
            None
        } else if self.degree() < divisor.degree() {
            Some((Self::zero(), self.clone()))
        } else {
            // Now we know that self.degree() >= divisor.degree();
            let mut quotient = vec![F::zero(); self.degree() - divisor.degree() + 1];
            let mut remainder: Self = self.clone();
            // Can unwrap here because we know self is not zero.
            let divisor_leading_inv = divisor.leading_coefficient().unwrap().inverse().unwrap();
            while !remainder.is_zero() && remainder.degree() >= divisor.degree() {
                let cur_q_coeff = *remainder.leading_coefficient().unwrap() * divisor_leading_inv;
                let cur_q_degree = remainder.degree() - divisor.degree();
                quotient[cur_q_degree] = cur_q_coeff;

                for (i, div_coeff) in divisor.coeffs.iter().enumerate() {
                    remainder.coeffs[cur_q_degree + i] -= &(cur_q_coeff * div_coeff);
                }
                while let Some(true) = remainder.coeffs.last().map(|c| c == &F::zero()) {
                    remainder.coeffs.pop();
                }
            }
            Some((Self::from_coeff(quotient), remainder))
        }
    }

    fn is_zero(&self) -> bool {
        self.coeffs.is_empty() || self.coeffs.iter().all(|c| c == &F::zero())
    }

    fn leading_coefficient(&self) -> Option<&F> {
        self.coeffs.last()
    }

    fn zero() -> Self {
        Self::from_coeff(Vec::new())
    }

    pub fn degree(&self) -> usize {
        self.coeffs.len() - 1
    }

    pub fn as_vec(&self) -> Vec<F> {
        self.coeffs.clone()
    }

    pub fn eval_at_zero(&self) -> F {
        self.coeffs[0]
    }

    pub fn eval_at_one(&self) -> F {
        (0..self.coeffs.len()).map(|i| self.coeffs[i]).sum()
    }

    pub fn evaluate(&self, r: &F) -> F {
        let mut eval = self.coeffs[0];
        let mut power = *r;
        for i in 1..self.coeffs.len() {
            eval += power * self.coeffs[i];
            power *= r;
        }
        eval
    }

    pub fn compress(&self) -> CompressedUniPoly<F> {
        let coeffs_except_linear_term = [&self.coeffs[..1], &self.coeffs[2..]].concat();
        debug_assert_eq!(coeffs_except_linear_term.len() + 1, self.coeffs.len());
        CompressedUniPoly {
            coeffs_except_linear_term,
        }
    }
}

impl<F: JoltField> CompressedUniPoly<F> {
    // we require eval(0) + eval(1) = hint, so we can solve for the linear term as:
    // linear_term = hint - 2 * constant_term - deg2 term - deg3 term
    pub fn decompress(&self, hint: &F) -> UniPoly<F> {
        let mut linear_term =
            *hint - self.coeffs_except_linear_term[0] - self.coeffs_except_linear_term[0];
        for i in 1..self.coeffs_except_linear_term.len() {
            linear_term -= self.coeffs_except_linear_term[i];
        }

        let mut coeffs = vec![self.coeffs_except_linear_term[0], linear_term];
        coeffs.extend(&self.coeffs_except_linear_term[1..]);
        assert_eq!(self.coeffs_except_linear_term.len() + 1, coeffs.len());
        UniPoly { coeffs }
    }
}

impl<F: JoltField> AppendToTranscript for UniPoly<F> {
    fn append_to_transcript(&self, label: &'static [u8], transcript: &mut ProofTranscript) {
        transcript.append_message(label, b"UniPoly_begin");
        for i in 0..self.coeffs.len() {
            transcript.append_scalar(b"coeff", &self.coeffs[i]);
        }
        transcript.append_message(label, b"UniPoly_end");
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use ark_bn254::Fr;

    #[test]
    fn test_from_evals_quad() {
        test_from_evals_quad_helper::<Fr>()
    }

    fn test_from_evals_quad_helper<F: JoltField>() {
        // polynomial is 2x^2 + 3x + 1
        let e0 = F::one();
        let e1 = F::from_u64(6u64).unwrap();
        let e2 = F::from_u64(15u64).unwrap();
        let evals = vec![e0, e1, e2];
        let poly = UniPoly::from_evals(&evals);

        assert_eq!(poly.eval_at_zero(), e0);
        assert_eq!(poly.eval_at_one(), e1);
        assert_eq!(poly.coeffs.len(), 3);
        assert_eq!(poly.coeffs[0], F::one());
        assert_eq!(poly.coeffs[1], F::from_u64(3u64).unwrap());
        assert_eq!(poly.coeffs[2], F::from_u64(2u64).unwrap());

        let hint = e0 + e1;
        let compressed_poly = poly.compress();
        let decompressed_poly = compressed_poly.decompress(&hint);
        for i in 0..decompressed_poly.coeffs.len() {
            assert_eq!(decompressed_poly.coeffs[i], poly.coeffs[i]);
        }

        let e3 = F::from_u64(28u64).unwrap();
        assert_eq!(poly.evaluate(&F::from_u64(3u64).unwrap()), e3);
    }

    #[test]
    fn test_from_evals_cubic() {
        test_from_evals_cubic_helper::<Fr>()
    }
    fn test_from_evals_cubic_helper<F: JoltField>() {
        // polynomial is x^3 + 2x^2 + 3x + 1
        let e0 = F::one();
        let e1 = F::from_u64(7u64).unwrap();
        let e2 = F::from_u64(23u64).unwrap();
        let e3 = F::from_u64(55u64).unwrap();
        let evals = vec![e0, e1, e2, e3];
        let poly = UniPoly::from_evals(&evals);

        assert_eq!(poly.eval_at_zero(), e0);
        assert_eq!(poly.eval_at_one(), e1);
        assert_eq!(poly.coeffs.len(), 4);
        assert_eq!(poly.coeffs[0], F::one());
        assert_eq!(poly.coeffs[1], F::from_u64(3u64).unwrap());
        assert_eq!(poly.coeffs[2], F::from_u64(2u64).unwrap());
        assert_eq!(poly.coeffs[3], F::one());

        let hint = e0 + e1;
        let compressed_poly = poly.compress();
        let decompressed_poly = compressed_poly.decompress(&hint);
        for i in 0..decompressed_poly.coeffs.len() {
            assert_eq!(decompressed_poly.coeffs[i], poly.coeffs[i]);
        }

        let e4 = F::from_u64(109u64).unwrap();
        assert_eq!(poly.evaluate(&F::from_u64(4u64).unwrap()), e4);
    }
}
