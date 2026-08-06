use thiserror::Error;

use super::operator::{LinearOperator, OperatorError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LsmrOptions {
    pub damping: f64,
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
    pub condition_limit: f64,

    pub max_iterations: Option<usize>,
}

impl Default for LsmrOptions {
    fn default() -> Self {
        Self {
            damping: 0.0,
            absolute_tolerance: 1.0e-6,
            relative_tolerance: 1.0e-6,
            condition_limit: 1.0e8,
            max_iterations: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LsmrStop {
    ZeroSolution = 0,
    CompatibleSystem = 1,
    LeastSquaresSolution = 2,
    ConditionLimit = 3,
    CompatibleMachinePrecision = 4,
    LeastSquaresMachinePrecision = 5,
    ConditionMachinePrecision = 6,
    IterationLimit = 7,
}

impl LsmrStop {
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LsmrReport {
    pub solution: Vec<f64>,
    pub stop: LsmrStop,
    pub iterations: usize,
    pub residual_norm: f64,
    pub normal_residual_norm: f64,
    pub operator_norm: f64,
    pub condition_estimate: f64,
    pub solution_norm: f64,
}

#[derive(Debug, Error, PartialEq)]
pub enum LsmrError {
    #[error("LSMR requires an operator with at least one row and one column")]
    EmptyOperator,
    #[error("right-hand side length {actual} does not match operator rows {expected}")]
    RightHandSideLength { expected: usize, actual: usize },
    #[error("right-hand side entry {0} is not finite")]
    NonFiniteRightHandSide(usize),
    #[error("LSMR option {0} must be finite and non-negative")]
    InvalidOption(&'static str),
    #[error("LSMR max_iterations must be positive")]
    ZeroIterationLimit,
    #[error("LSMR recurrence produced a non-finite value at iteration {0}")]
    NumericalBreakdown(usize),
    #[error(transparent)]
    Operator(#[from] OperatorError),
}

#[must_use]
pub fn sym_ortho(a: f64, b: f64) -> (f64, f64, f64) {
    if b == 0.0 {
        let sign = if a > 0.0 {
            1.0
        } else if a < 0.0 {
            -1.0
        } else {
            0.0
        };
        (sign, 0.0, a.abs())
    } else if a == 0.0 {
        (0.0, b.signum(), b.abs())
    } else if b.abs() > a.abs() {
        let tau = a / b;
        let s = b.signum() / (1.0 + tau * tau).sqrt();
        let c = s * tau;
        (c, s, b / s)
    } else {
        let tau = b / a;
        let c = a.signum() / (1.0 + tau * tau).sqrt();
        let s = c * tau;
        (c, s, a / c)
    }
}

fn norm2(values: &[f64]) -> f64 {
    let mut scale = 0.0;
    let mut scaled_sum = 1.0;
    for value in values.iter().copied() {
        let absolute = value.abs();
        if absolute == 0.0 {
            continue;
        }
        if scale < absolute {
            let ratio = scale / absolute;
            scaled_sum = 1.0 + scaled_sum * ratio * ratio;
            scale = absolute;
        } else {
            let ratio = absolute / scale;
            scaled_sum += ratio * ratio;
        }
    }
    if scale == 0.0 {
        0.0
    } else {
        scale * scaled_sum.sqrt()
    }
}

fn validate_options(options: LsmrOptions) -> Result<(), LsmrError> {
    for (name, value) in [
        ("damping", options.damping),
        ("absolute_tolerance", options.absolute_tolerance),
        ("relative_tolerance", options.relative_tolerance),
        ("condition_limit", options.condition_limit),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(LsmrError::InvalidOption(name));
        }
    }
    if options.max_iterations == Some(0) {
        return Err(LsmrError::ZeroIterationLimit);
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
pub fn lsmr<O: LinearOperator>(
    operator: &O,
    right_hand_side: &[f64],
    options: LsmrOptions,
) -> Result<LsmrReport, LsmrError> {
    validate_options(options)?;
    let rows = operator.rows();
    let columns = operator.columns();
    if rows == 0 || columns == 0 {
        return Err(LsmrError::EmptyOperator);
    }
    if right_hand_side.len() != rows {
        return Err(LsmrError::RightHandSideLength {
            expected: rows,
            actual: right_hand_side.len(),
        });
    }
    if let Some(index) = right_hand_side.iter().position(|value| !value.is_finite()) {
        return Err(LsmrError::NonFiniteRightHandSide(index));
    }
    let max_iterations = options.max_iterations.unwrap_or(rows.min(columns));

    let mut u = right_hand_side.to_vec();
    let norm_b = norm2(right_hand_side);
    let mut x = vec![0.0; columns];
    let mut beta = norm_b;
    let mut v = vec![0.0; columns];
    let mut alpha = 0.0;
    if beta > 0.0 {
        for value in &mut u {
            *value /= beta;
        }
        operator.apply_transpose(&u, &mut v)?;
        alpha = norm2(&v);
    }
    if alpha > 0.0 {
        for value in &mut v {
            *value /= alpha;
        }
    }

    let mut iteration = 0;
    let mut zeta_bar = alpha * beta;
    let mut alpha_bar = alpha;
    let mut rho = 1.0;
    let mut rho_bar = 1.0;
    let mut c_bar = 1.0;
    let mut s_bar = 0.0;
    let mut h = v.clone();
    let mut h_bar = vec![0.0; columns];

    let mut beta_dd = beta;
    let mut beta_d = 0.0;
    let mut rho_d_old = 1.0;
    let mut tau_tilde_old = 0.0;
    let mut theta_tilde = 0.0;
    let mut zeta = 0.0;
    let mut d = 0.0;

    let mut norm_a_squared = alpha * alpha;
    let mut max_r_bar: f64 = 0.0;
    let mut min_r_bar: f64 = 1.0e100;
    let mut norm_a = norm_a_squared.sqrt();
    let mut condition_a = 1.0;
    let mut norm_x = 0.0;
    let mut norm_r = beta;
    let mut norm_ar = alpha * beta;

    let early_report = |solution: Vec<f64>, residual_norm: f64, operator_norm: f64| LsmrReport {
        solution,
        stop: LsmrStop::ZeroSolution,
        iterations: 0,
        residual_norm,
        normal_residual_norm: norm_ar,
        operator_norm,
        condition_estimate: 1.0,
        solution_norm: 0.0,
    };
    if norm_ar == 0.0 {
        return Ok(early_report(x, norm_r, norm_a));
    }
    if norm_b == 0.0 {
        return Ok(early_report(vec![0.0; columns], 0.0, norm_a));
    }

    let condition_tolerance = if options.condition_limit > 0.0 {
        1.0 / options.condition_limit
    } else {
        0.0
    };
    let mut stop = LsmrStop::IterationLimit;
    let mut a_v = vec![0.0; rows];
    let mut at_u = vec![0.0; columns];

    while iteration < max_iterations {
        iteration += 1;

        operator.apply(&v, &mut a_v)?;
        for index in 0..rows {
            u[index] = a_v[index] - alpha * u[index];
        }
        beta = norm2(&u);
        if beta > 0.0 {
            for value in &mut u {
                *value /= beta;
            }
            operator.apply_transpose(&u, &mut at_u)?;
            for index in 0..columns {
                v[index] = at_u[index] - beta * v[index];
            }
            alpha = norm2(&v);
            if alpha > 0.0 {
                for value in &mut v {
                    *value /= alpha;
                }
            }
        }

        let (c_hat, s_hat, alpha_hat) = sym_ortho(alpha_bar, options.damping);
        let rho_old = rho;
        let (c, s, new_rho) = sym_ortho(alpha_hat, beta);
        rho = new_rho;
        let theta_new = s * alpha;
        alpha_bar = c * alpha;

        let rho_bar_old = rho_bar;
        let zeta_old = zeta;
        let theta_bar = s_bar * rho;
        let rho_temp = c_bar * rho;
        let (new_c_bar, new_s_bar, new_rho_bar) = sym_ortho(c_bar * rho, theta_new);
        c_bar = new_c_bar;
        s_bar = new_s_bar;
        rho_bar = new_rho_bar;
        zeta = c_bar * zeta_bar;
        zeta_bar *= -s_bar;

        let h_bar_scale = -(theta_bar * rho / (rho_old * rho_bar_old));
        let x_scale = zeta / (rho * rho_bar);
        let h_scale = -(theta_new / rho);
        for index in 0..columns {
            h_bar[index] = h_bar_scale * h_bar[index] + h[index];
            x[index] += x_scale * h_bar[index];
            h[index] = h_scale * h[index] + v[index];
        }

        let beta_acute = c_hat * beta_dd;
        let beta_check = -s_hat * beta_dd;
        let beta_hat = c * beta_acute;
        beta_dd = -s * beta_acute;

        let theta_tilde_old = theta_tilde;
        let (c_tilde_old, s_tilde_old, rho_tilde_old) = sym_ortho(rho_d_old, theta_bar);
        theta_tilde = s_tilde_old * rho_bar;
        rho_d_old = c_tilde_old * rho_bar;
        beta_d = -s_tilde_old * beta_d + c_tilde_old * beta_hat;
        tau_tilde_old = (zeta_old - theta_tilde_old * tau_tilde_old) / rho_tilde_old;
        let tau_d = (zeta - theta_tilde * tau_tilde_old) / rho_d_old;
        d += beta_check * beta_check;
        norm_r = (d + (beta_d - tau_d).powi(2) + beta_dd * beta_dd).sqrt();

        norm_a_squared += beta * beta;
        norm_a = norm_a_squared.sqrt();
        norm_a_squared += alpha * alpha;
        max_r_bar = max_r_bar.max(rho_bar_old);
        if iteration > 1 {
            min_r_bar = min_r_bar.min(rho_bar_old);
        }
        condition_a = max_r_bar.max(rho_temp) / min_r_bar.min(rho_temp);

        norm_ar = zeta_bar.abs();
        norm_x = norm2(&x);
        let test_1 = norm_r / norm_b;
        let test_2 = if norm_a * norm_r != 0.0 {
            norm_ar / (norm_a * norm_r)
        } else {
            f64::INFINITY
        };
        let test_3 = 1.0 / condition_a;
        let transformed_test_1 = test_1 / (1.0 + norm_a * norm_x / norm_b);
        let residual_tolerance =
            options.relative_tolerance + options.absolute_tolerance * norm_a * norm_x / norm_b;

        let scalars = [
            beta,
            alpha,
            rho,
            rho_bar,
            norm_r,
            norm_ar,
            norm_a,
            condition_a,
            norm_x,
        ];
        if scalars.iter().any(|value| !value.is_finite())
            || x.iter().any(|value| !value.is_finite())
        {
            return Err(LsmrError::NumericalBreakdown(iteration));
        }

        let mut iteration_stop = None;
        if iteration >= max_iterations {
            iteration_stop = Some(LsmrStop::IterationLimit);
        }
        if 1.0 + test_3 <= 1.0 {
            iteration_stop = Some(LsmrStop::ConditionMachinePrecision);
        }
        if 1.0 + test_2 <= 1.0 {
            iteration_stop = Some(LsmrStop::LeastSquaresMachinePrecision);
        }
        if 1.0 + transformed_test_1 <= 1.0 {
            iteration_stop = Some(LsmrStop::CompatibleMachinePrecision);
        }
        if test_3 <= condition_tolerance {
            iteration_stop = Some(LsmrStop::ConditionLimit);
        }
        if test_2 <= options.absolute_tolerance {
            iteration_stop = Some(LsmrStop::LeastSquaresSolution);
        }
        if test_1 <= residual_tolerance {
            iteration_stop = Some(LsmrStop::CompatibleSystem);
        }
        if let Some(reason) = iteration_stop {
            stop = reason;
            break;
        }
    }

    Ok(LsmrReport {
        solution: x,
        stop,
        iterations: iteration,
        residual_norm: norm_r,
        normal_residual_norm: norm_ar,
        operator_norm: norm_a,
        condition_estimate: condition_a,
        solution_norm: norm_x,
    })
}
