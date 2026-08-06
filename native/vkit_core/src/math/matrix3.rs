use super::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    rows: [[f64; 3]; 3],
}

impl Mat3 {
    pub const IDENTITY: Self = Self::from_rows([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    #[must_use]
    pub const fn from_rows(rows: [[f64; 3]; 3]) -> Self {
        Self { rows }
    }

    #[must_use]
    pub const fn rows(self) -> [[f64; 3]; 3] {
        self.rows
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.rows.into_iter().flatten().all(f64::is_finite)
    }

    #[must_use]
    pub fn determinant(self) -> f64 {
        let m = self.rows;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    #[must_use]
    pub fn transpose(self) -> Self {
        let m = self.rows;
        Self::from_rows([
            [m[0][0], m[1][0], m[2][0]],
            [m[0][1], m[1][1], m[2][1]],
            [m[0][2], m[1][2], m[2][2]],
        ])
    }

    #[must_use]
    pub fn transform_vector(self, value: Vec3) -> Vec3 {
        let m = self.rows;
        Vec3::new(
            m[0][0] * value.x + m[0][1] * value.y + m[0][2] * value.z,
            m[1][0] * value.x + m[1][1] * value.y + m[1][2] * value.z,
            m[2][0] * value.x + m[2][1] * value.y + m[2][2] * value.z,
        )
    }

    pub(crate) fn from_na(value: nalgebra::Matrix3<f64>) -> Self {
        Self::from_rows([
            [value[(0, 0)], value[(0, 1)], value[(0, 2)]],
            [value[(1, 0)], value[(1, 1)], value[(1, 2)]],
            [value[(2, 0)], value[(2, 1)], value[(2, 2)]],
        ])
    }
}
