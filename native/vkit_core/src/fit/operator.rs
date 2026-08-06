use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum OperatorError {
    #[error("row-offset count {actual} does not equal rows + 1 ({expected})")]
    RowOffsetCount { expected: usize, actual: usize },
    #[error("the first CSR row offset must be zero")]
    NonZeroFirstOffset,
    #[error("CSR row offsets are not monotonic at position {0}")]
    NonMonotonicRowOffsets(usize),
    #[error("the final CSR offset {offset} does not equal the nonzero count {nonzeros}")]
    FinalOffsetMismatch { offset: usize, nonzeros: usize },
    #[error("column-index and value counts differ ({indices} != {values})")]
    NonzeroCountMismatch { indices: usize, values: usize },
    #[error("column index {column} in row {row} is outside 0..{columns}")]
    ColumnOutOfBounds {
        row: usize,
        column: usize,
        columns: usize,
    },
    #[error("column indices in CSR row {row} must be strictly increasing")]
    UnsortedColumns { row: usize },
    #[error("CSR value at nonzero position {0} is not finite")]
    NonFiniteValue(usize),
    #[error("triplet ({row}, {column}) is outside a {rows}x{columns} matrix")]
    TripletOutOfBounds {
        row: usize,
        column: usize,
        rows: usize,
        columns: usize,
    },
    #[error("operator {kind} vector has length {actual}; expected {expected}")]
    VectorLength {
        kind: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("a stacked operator requires at least one block")]
    EmptyStack,
    #[error("stacked operator block {block} has {actual} columns; expected {expected}")]
    StackColumnMismatch {
        block: usize,
        expected: usize,
        actual: usize,
    },
    #[error("stacked operator row count overflowed usize")]
    StackRowOverflow,
}

pub trait LinearOperator {
    fn rows(&self) -> usize;
    fn columns(&self) -> usize;

    fn apply(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError>;

    fn apply_transpose(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError>;

    fn apply_transpose_add(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        if output.len() != self.columns() {
            return Err(OperatorError::VectorLength {
                kind: "transpose accumulation output",
                expected: self.columns(),
                actual: output.len(),
            });
        }
        let mut contribution = vec![0.0; self.columns()];
        self.apply_transpose(input, &mut contribution)?;
        for (result, value) in output.iter_mut().zip(contribution) {
            *result += value;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct StackedOperator<'a> {
    blocks: Vec<&'a dyn LinearOperator>,
    rows: usize,
    columns: usize,
}

impl std::fmt::Debug for StackedOperator<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StackedOperator")
            .field("block_count", &self.blocks.len())
            .field("rows", &self.rows)
            .field("columns", &self.columns)
            .finish()
    }
}

impl<'a> StackedOperator<'a> {
    pub fn new(blocks: Vec<&'a dyn LinearOperator>) -> Result<Self, OperatorError> {
        let Some(first) = blocks.first() else {
            return Err(OperatorError::EmptyStack);
        };
        let columns = first.columns();
        let mut rows = 0_usize;
        for (block, operator) in blocks.iter().enumerate() {
            if operator.columns() != columns {
                return Err(OperatorError::StackColumnMismatch {
                    block,
                    expected: columns,
                    actual: operator.columns(),
                });
            }
            rows = rows
                .checked_add(operator.rows())
                .ok_or(OperatorError::StackRowOverflow)?;
        }
        Ok(Self {
            blocks,
            rows,
            columns,
        })
    }

    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }
}

impl LinearOperator for StackedOperator<'_> {
    fn rows(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.columns
    }

    fn apply(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        if input.len() != self.columns {
            return Err(OperatorError::VectorLength {
                kind: "stack input",
                expected: self.columns,
                actual: input.len(),
            });
        }
        if output.len() != self.rows {
            return Err(OperatorError::VectorLength {
                kind: "stack output",
                expected: self.rows,
                actual: output.len(),
            });
        }
        let mut offset = 0;
        for block in &self.blocks {
            let end = offset + block.rows();
            block.apply(input, &mut output[offset..end])?;
            offset = end;
        }
        Ok(())
    }

    fn apply_transpose(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        if input.len() != self.rows {
            return Err(OperatorError::VectorLength {
                kind: "stack transpose input",
                expected: self.rows,
                actual: input.len(),
            });
        }
        if output.len() != self.columns {
            return Err(OperatorError::VectorLength {
                kind: "stack transpose output",
                expected: self.columns,
                actual: output.len(),
            });
        }
        output.fill(0.0);
        let mut offset = 0;
        for block in &self.blocks {
            let end = offset + block.rows();
            block.apply_transpose_add(&input[offset..end], output)?;
            offset = end;
        }
        Ok(())
    }

    fn apply_transpose_add(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        if input.len() != self.rows {
            return Err(OperatorError::VectorLength {
                kind: "stack transpose accumulation input",
                expected: self.rows,
                actual: input.len(),
            });
        }
        if output.len() != self.columns {
            return Err(OperatorError::VectorLength {
                kind: "stack transpose accumulation output",
                expected: self.columns,
                actual: output.len(),
            });
        }
        let mut offset = 0;
        for block in &self.blocks {
            let end = offset + block.rows();
            block.apply_transpose_add(&input[offset..end], output)?;
            offset = end;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CsrMatrix {
    rows: usize,
    columns: usize,
    row_offsets: Vec<usize>,
    column_indices: Vec<usize>,
    values: Vec<f64>,
}

impl CsrMatrix {
    pub fn new(
        rows: usize,
        columns: usize,
        row_offsets: Vec<usize>,
        column_indices: Vec<usize>,
        values: Vec<f64>,
    ) -> Result<Self, OperatorError> {
        if row_offsets.len() != rows + 1 {
            return Err(OperatorError::RowOffsetCount {
                expected: rows + 1,
                actual: row_offsets.len(),
            });
        }
        if row_offsets.first().copied() != Some(0) {
            return Err(OperatorError::NonZeroFirstOffset);
        }
        if column_indices.len() != values.len() {
            return Err(OperatorError::NonzeroCountMismatch {
                indices: column_indices.len(),
                values: values.len(),
            });
        }
        for index in 1..row_offsets.len() {
            if row_offsets[index] < row_offsets[index - 1] {
                return Err(OperatorError::NonMonotonicRowOffsets(index));
            }
        }
        let final_offset = row_offsets.last().copied().unwrap_or(0);
        if final_offset != values.len() {
            return Err(OperatorError::FinalOffsetMismatch {
                offset: final_offset,
                nonzeros: values.len(),
            });
        }
        for (index, value) in values.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(OperatorError::NonFiniteValue(index));
            }
        }
        for row in 0..rows {
            let range = row_offsets[row]..row_offsets[row + 1];
            let mut previous = None;
            for &column in &column_indices[range] {
                if column >= columns {
                    return Err(OperatorError::ColumnOutOfBounds {
                        row,
                        column,
                        columns,
                    });
                }
                if previous.is_some_and(|value| column <= value) {
                    return Err(OperatorError::UnsortedColumns { row });
                }
                previous = Some(column);
            }
        }
        Ok(Self {
            rows,
            columns,
            row_offsets,
            column_indices,
            values,
        })
    }

    pub fn from_triplets<I>(rows: usize, columns: usize, triplets: I) -> Result<Self, OperatorError>
    where
        I: IntoIterator<Item = (usize, usize, f64)>,
    {
        let mut entries = Vec::new();
        for (ordinal, (row, column, value)) in triplets.into_iter().enumerate() {
            if row >= rows || column >= columns {
                return Err(OperatorError::TripletOutOfBounds {
                    row,
                    column,
                    rows,
                    columns,
                });
            }
            if !value.is_finite() {
                return Err(OperatorError::NonFiniteValue(ordinal));
            }
            entries.push((row, column, value));
        }

        entries.sort_by_key(|&(row, column, _)| (row, column));

        let mut combined: Vec<(usize, usize, f64)> = Vec::with_capacity(entries.len());
        for (row, column, value) in entries {
            if let Some(last) = combined.last_mut()
                && last.0 == row
                && last.1 == column
            {
                last.2 += value;
            } else {
                combined.push((row, column, value));
            }
        }

        let mut row_offsets = vec![0; rows + 1];
        for &(row, _, _) in &combined {
            row_offsets[row + 1] += 1;
        }
        for row in 0..rows {
            row_offsets[row + 1] += row_offsets[row];
        }
        let mut column_indices = Vec::with_capacity(combined.len());
        let mut values = Vec::with_capacity(combined.len());
        for (_, column, value) in combined {
            column_indices.push(column);
            values.push(value);
        }
        Self::new(rows, columns, row_offsets, column_indices, values)
    }

    #[must_use]
    pub fn nonzero_count(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn row_offsets(&self) -> &[usize] {
        &self.row_offsets
    }

    #[must_use]
    pub fn column_indices(&self) -> &[usize] {
        &self.column_indices
    }

    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }
}

impl LinearOperator for CsrMatrix {
    fn rows(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.columns
    }

    fn apply(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        if input.len() != self.columns {
            return Err(OperatorError::VectorLength {
                kind: "input",
                expected: self.columns,
                actual: input.len(),
            });
        }
        if output.len() != self.rows {
            return Err(OperatorError::VectorLength {
                kind: "output",
                expected: self.rows,
                actual: output.len(),
            });
        }
        for (row, result) in output.iter_mut().enumerate() {
            let mut sum = 0.0;
            for position in self.row_offsets[row]..self.row_offsets[row + 1] {
                sum += self.values[position] * input[self.column_indices[position]];
            }
            *result = sum;
        }
        Ok(())
    }

    fn apply_transpose(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        if input.len() != self.rows {
            return Err(OperatorError::VectorLength {
                kind: "transpose input",
                expected: self.rows,
                actual: input.len(),
            });
        }
        if output.len() != self.columns {
            return Err(OperatorError::VectorLength {
                kind: "transpose output",
                expected: self.columns,
                actual: output.len(),
            });
        }
        output.fill(0.0);
        for (row, &input_value) in input.iter().enumerate() {
            for position in self.row_offsets[row]..self.row_offsets[row + 1] {
                output[self.column_indices[position]] += self.values[position] * input_value;
            }
        }
        Ok(())
    }

    fn apply_transpose_add(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        if input.len() != self.rows {
            return Err(OperatorError::VectorLength {
                kind: "transpose accumulation input",
                expected: self.rows,
                actual: input.len(),
            });
        }
        if output.len() != self.columns {
            return Err(OperatorError::VectorLength {
                kind: "transpose accumulation output",
                expected: self.columns,
                actual: output.len(),
            });
        }
        for (row, &input_value) in input.iter().enumerate() {
            for position in self.row_offsets[row]..self.row_offsets[row + 1] {
                output[self.column_indices[position]] += self.values[position] * input_value;
            }
        }
        Ok(())
    }
}
