use approx::assert_abs_diff_eq;
use vkit_core::fit::{
    CsrMatrix, LinearOperator, LsmrError, LsmrOptions, LsmrStop, OperatorError, lsmr, sym_ortho,
};

fn strict_options() -> LsmrOptions {
    LsmrOptions {
        damping: 0.0,
        absolute_tolerance: 1.0e-12,
        relative_tolerance: 1.0e-12,
        condition_limit: 1.0e12,
        max_iterations: Some(100),
    }
}

fn csr_from_dense(rows: &[&[f64]]) -> CsrMatrix {
    let column_count = rows.first().map_or(0, |row| row.len());
    let mut triplets = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        assert_eq!(row.len(), column_count);
        for (column, value) in row.iter().copied().enumerate() {
            if value != 0.0 {
                triplets.push((row_index, column, value));
            }
        }
    }
    CsrMatrix::from_triplets(rows.len(), column_count, triplets).expect("valid dense matrix")
}

#[test]
fn csr_products_match_hand_computation() {
    let matrix = csr_from_dense(&[&[1.0, 0.0, 2.0], &[0.0, -3.0, 0.0], &[4.0, 5.0, 6.0]]);
    let mut product = vec![99.0; 3];
    matrix.apply(&[2.0, -1.0, 0.5], &mut product).unwrap();
    assert_eq!(product, vec![3.0, 3.0, 6.0]);

    let mut transpose_product = vec![99.0; 3];
    matrix
        .apply_transpose(&[2.0, -1.0, 3.0], &mut transpose_product)
        .unwrap();
    assert_eq!(transpose_product, vec![14.0, 18.0, 22.0]);
}

#[test]
fn triplets_are_sorted_and_duplicates_are_combined_deterministically() {
    let matrix =
        CsrMatrix::from_triplets(2, 3, [(1, 2, 4.0), (0, 1, 1.5), (1, 0, -2.0), (0, 1, 0.5)])
            .unwrap();
    assert_eq!(matrix.row_offsets(), &[0, 1, 3]);
    assert_eq!(matrix.column_indices(), &[1, 0, 2]);
    assert_eq!(matrix.values(), &[2.0, -2.0, 4.0]);
    assert_eq!(matrix.nonzero_count(), 3);
}

#[test]
fn csr_rejects_ambiguous_or_unsafe_storage() {
    let error = CsrMatrix::new(1, 3, vec![0, 2], vec![1, 1], vec![1.0, 2.0])
        .expect_err("duplicate columns are not canonical");
    assert_eq!(error, OperatorError::UnsortedColumns { row: 0 });

    let error =
        CsrMatrix::from_triplets(1, 1, [(0, 1, 2.0)]).expect_err("column is outside the matrix");
    assert!(matches!(error, OperatorError::TripletOutOfBounds { .. }));
}

#[test]
fn compatible_system_matches_scipy_oracle() {
    let matrix = csr_from_dense(&[&[3.0, 0.0], &[0.0, 4.0], &[1.0, 0.0], &[0.0, 1.0]]);
    let report = lsmr(&matrix, &[6.0, -12.0, 2.0, -3.0], strict_options()).unwrap();
    assert_eq!(report.stop, LsmrStop::CompatibleSystem);
    assert_eq!(report.iterations, 2);
    assert_abs_diff_eq!(report.solution[0], 2.0, epsilon = 2.0e-14);
    assert_abs_diff_eq!(report.solution[1], -3.0, epsilon = 2.0e-14);
    assert_abs_diff_eq!(
        report.operator_norm,
        5.196_152_422_706_632,
        epsilon = 2.0e-13
    );
    assert_abs_diff_eq!(
        report.condition_estimate,
        1.259_302_206_805_868_6,
        epsilon = 2.0e-13
    );
}

#[test]
fn inconsistent_system_matches_scipy_least_squares_solution() {
    let matrix = csr_from_dense(&[&[1.0, 0.0], &[0.0, 1.0], &[1.0, 1.0]]);
    let report = lsmr(&matrix, &[1.0, 2.0, 4.0], strict_options()).unwrap();
    assert_eq!(report.stop, LsmrStop::LeastSquaresSolution);
    assert_eq!(report.iterations, 2);
    assert_abs_diff_eq!(report.solution[0], 4.0 / 3.0, epsilon = 3.0e-14);
    assert_abs_diff_eq!(report.solution[1], 7.0 / 3.0, epsilon = 3.0e-14);
    assert_abs_diff_eq!(
        report.residual_norm,
        1.0 / 3.0_f64.sqrt(),
        epsilon = 3.0e-14
    );
}

#[test]
fn general_rectangular_system_matches_scipy_oracle() {
    let matrix = csr_from_dense(&[
        &[1.0, 2.0, 0.0],
        &[-1.0, 0.0, 3.0],
        &[0.5, -2.0, 1.0],
        &[4.0, 1.0, -1.0],
        &[0.0, 3.0, 2.0],
    ]);
    let report = lsmr(&matrix, &[1.0, -2.0, 0.5, 3.0, 4.0], strict_options()).unwrap();
    assert_eq!(report.stop, LsmrStop::LeastSquaresSolution);
    assert_eq!(report.iterations, 3);
    let expected = [
        0.699_803_364_649_333_5,
        0.671_910_276_017_770_1,
        0.135_532_736_144_490_55,
    ];
    for (actual, wanted) in report.solution.iter().zip(expected) {
        assert_abs_diff_eq!(*actual, wanted, epsilon = 2.0e-13);
    }
    assert_abs_diff_eq!(
        report.residual_norm,
        2.982_482_295_150_398,
        epsilon = 2.0e-13
    );
}

#[test]
fn ill_scaled_system_remains_finite_and_matches_solution() {
    let matrix = csr_from_dense(&[&[1.0e-6, 0.0], &[0.0, 1.0e6], &[1.0, 1.0]]);
    let report = lsmr(&matrix, &[1.0e-6, 2.0e6, 3.0], strict_options()).unwrap();
    assert_eq!(report.stop, LsmrStop::CompatibleSystem);
    assert_abs_diff_eq!(report.solution[0], 1.0, epsilon = 2.0e-9);
    assert_abs_diff_eq!(report.solution[1], 2.0, epsilon = 2.0e-12);
    assert!(report.condition_estimate.is_finite());
    assert!(report.operator_norm.is_finite());
}

#[test]
fn damped_solution_matches_scipy_oracle() {
    let matrix = csr_from_dense(&[&[1.0, 0.0], &[0.0, 2.0], &[1.0, 1.0]]);
    let mut options = strict_options();
    options.damping = 0.5;
    let report = lsmr(&matrix, &[1.0, 2.0, 4.0], options).unwrap();
    assert_eq!(report.stop, LsmrStop::LeastSquaresSolution);
    assert_eq!(report.iterations, 2);
    assert_abs_diff_eq!(
        report.solution[0],
        1.687_861_271_676_301_3,
        epsilon = 3.0e-13
    );
    assert_abs_diff_eq!(report.solution[1], 1.202_312_138_728_325, epsilon = 3.0e-13);
    assert_abs_diff_eq!(
        report.residual_norm,
        1.715_283_222_034_165_2,
        epsilon = 3.0e-13
    );
}

#[test]
fn rank_deficient_system_converges_without_normal_equations() {
    let matrix = csr_from_dense(&[&[1.0, 2.0], &[2.0, 4.0], &[3.0, 6.0]]);
    let report = lsmr(&matrix, &[1.0, 0.0, 0.0], strict_options()).unwrap();
    assert_eq!(report.stop, LsmrStop::LeastSquaresSolution);
    assert_eq!(report.iterations, 1);
    assert_abs_diff_eq!(report.solution[0], 1.0 / 70.0, epsilon = 2.0e-15);
    assert_abs_diff_eq!(report.solution[1], 1.0 / 35.0, epsilon = 2.0e-15);
}

#[test]
fn zero_right_hand_side_returns_without_iteration() {
    let matrix = csr_from_dense(&[&[1.0, 2.0], &[3.0, 4.0]]);
    let report = lsmr(&matrix, &[0.0, 0.0], strict_options()).unwrap();
    assert_eq!(report.stop, LsmrStop::ZeroSolution);
    assert_eq!(report.iterations, 0);
    assert_eq!(report.solution, vec![0.0, 0.0]);
}

#[test]
fn iteration_limit_is_reported_without_hiding_partial_solution() {
    let matrix = csr_from_dense(&[
        &[1.0, 2.0, 0.0],
        &[-1.0, 0.0, 3.0],
        &[0.5, -2.0, 1.0],
        &[4.0, 1.0, -1.0],
    ]);
    let mut options = strict_options();
    options.max_iterations = Some(1);
    let report = lsmr(&matrix, &[1.0, -2.0, 0.5, 3.0], options).unwrap();
    assert_eq!(report.stop, LsmrStop::IterationLimit);
    assert_eq!(report.iterations, 1);
    assert!(report.solution.iter().any(|value| *value != 0.0));
}

#[test]
fn repeated_solves_are_bitwise_deterministic() {
    let matrix = csr_from_dense(&[
        &[1.0, 2.0, 0.0],
        &[-1.0, 0.0, 3.0],
        &[0.5, -2.0, 1.0],
        &[4.0, 1.0, -1.0],
        &[0.0, 3.0, 2.0],
    ]);
    let first = lsmr(&matrix, &[1.0, -2.0, 0.5, 3.0, 4.0], strict_options()).unwrap();
    for _ in 0..20 {
        let next = lsmr(&matrix, &[1.0, -2.0, 0.5, 3.0, 4.0], strict_options()).unwrap();
        assert_eq!(next, first);
    }
}

#[test]
fn invalid_lsmr_inputs_return_typed_errors() {
    let matrix = csr_from_dense(&[&[1.0, 0.0], &[0.0, 1.0]]);
    assert_eq!(
        lsmr(&matrix, &[1.0], strict_options()),
        Err(LsmrError::RightHandSideLength {
            expected: 2,
            actual: 1,
        })
    );
    let mut invalid = strict_options();
    invalid.absolute_tolerance = f64::NAN;
    assert_eq!(
        lsmr(&matrix, &[1.0, 2.0], invalid),
        Err(LsmrError::InvalidOption("absolute_tolerance"))
    );
}

#[test]
fn sym_ortho_is_stable_at_axis_and_extreme_ratios() {
    for (a, b) in [(3.0, 0.0), (0.0, -4.0), (1.0e-200, 1.0e200), (-4.0, 3.0)] {
        let (c, s, r) = sym_ortho(a, b);
        assert_abs_diff_eq!(c * c + s * s, 1.0, epsilon = 2.0e-15);
        assert_abs_diff_eq!(c * a + s * b, r, epsilon = r.abs() * 2.0e-15 + 1.0e-300);
        assert_abs_diff_eq!(
            -s * a + c * b,
            0.0,
            epsilon = (a.abs() + b.abs()) * 2.0e-15 + 1.0e-300
        );
    }
    assert_eq!(sym_ortho(0.0, 0.0), (0.0, 0.0, 0.0));
}
