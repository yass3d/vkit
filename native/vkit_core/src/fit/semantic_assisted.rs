use std::{collections::BTreeSet, fmt};

use vkit_semantic::{SEMANTIC_LANDMARK_COUNT, detect_landmarks_rgb_f32};

use crate::formats::{Mesh, SurfaceAttachment};
use crate::math::{SimilarityTransform, Vec3};

use super::dense_registration::largest_component_triangle_ids;
use super::geometry_assisted::{
    AutomaticConstraintRegion, AutomaticSurfaceConstraintOptions, AutomaticSurfaceCorrespondence,
};
use super::semantic_surface::{attach_visible_semantic_points, render_semantic_face};

const FACE_OVAL_LANDMARK_COUNT: usize = 468;
const MINIMUM_SEMANTIC_PAIRS: usize = 160;
const MINIMUM_PRESENCE_PROBABILITY: f32 = 0.50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticConstraintRejection {
    InvalidSurface,
    RenderFailed,
    InferenceFailed,
    FaceNotPresent,
    InsufficientSurfacePairs,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSurfaceConstraintReceipt {
    pub model_landmark_count: usize,
    pub usable_landmark_count: usize,
    pub scan_presence_probability: Option<f32>,
    pub template_presence_probability: Option<f32>,
    pub scan_surface_hits: usize,
    pub template_surface_hits: usize,
    pub candidate_pairs: usize,
    pub distance_normal_rejections: usize,
    pub trimmed_outliers: usize,
    pub duplicate_scan_primitives_removed: usize,
    pub duplicate_template_primitives_removed: usize,
    pub accepted_pairs: usize,
    pub coverage: f64,
    pub rms_distance_cm: Option<f64>,
    pub maximum_distance_cm: Option<f64>,
    pub normals_were_flipped: bool,

    pub regional_constraint_counts: [usize; 6],
    pub regional_alignment_shares: [f64; 6],
    pub regional_fit_shares: [f64; 6],
    pub rejection_reason: Option<SemanticConstraintRejection>,
    pub failure_detail: Option<String>,
}

impl SemanticSurfaceConstraintReceipt {
    fn new() -> Self {
        Self {
            model_landmark_count: SEMANTIC_LANDMARK_COUNT,
            usable_landmark_count: FACE_OVAL_LANDMARK_COUNT,
            scan_presence_probability: None,
            template_presence_probability: None,
            scan_surface_hits: 0,
            template_surface_hits: 0,
            candidate_pairs: 0,
            distance_normal_rejections: 0,
            trimmed_outliers: 0,
            duplicate_scan_primitives_removed: 0,
            duplicate_template_primitives_removed: 0,
            accepted_pairs: 0,
            coverage: 0.0,
            rms_distance_cm: None,
            maximum_distance_cm: None,
            normals_were_flipped: false,
            regional_constraint_counts: [0; 6],
            regional_alignment_shares: [0.0; 6],
            regional_fit_shares: [0.0; 6],
            rejection_reason: None,
            failure_detail: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSurfaceConstraintResult {
    pub correspondences: Vec<AutomaticSurfaceCorrespondence>,
    pub receipt: SemanticSurfaceConstraintReceipt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticSurfaceConstraintError {
    pub receipt: Box<SemanticSurfaceConstraintReceipt>,
}

impl fmt::Display for SemanticSurfaceConstraintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "semantic surface constraints rejected: {:?}",
            self.receipt.rejection_reason
        )?;
        if let Some(detail) = &self.receipt.failure_detail {
            write!(formatter, " ({detail})")?;
        }
        Ok(())
    }
}

impl std::error::Error for SemanticSurfaceConstraintError {}

pub fn build_semantic_surface_correspondences(
    scan: &Mesh,
    template: &Mesh,
    template_visual_triangles: &[u32],
    template_skin_triangles: &[u32],
    scan_to_template: SimilarityTransform,
) -> Result<SemanticSurfaceConstraintResult, SemanticSurfaceConstraintError> {
    let options = AutomaticSurfaceConstraintOptions::default();
    let mut receipt = SemanticSurfaceConstraintReceipt::new();
    if scan.require_surface().is_err()
        || template.require_surface().is_err()
        || template_visual_triangles.is_empty()
        || template_skin_triangles.is_empty()
    {
        return semantic_rejection(receipt, SemanticConstraintRejection::InvalidSurface, None);
    }
    let scan_triangles = (0..scan.triangles.len() as u32).collect::<Vec<_>>();
    let scan_render =
        render_semantic_face(scan, &scan_triangles, scan_to_template).map_err(|error| {
            semantic_error(
                receipt.clone(),
                SemanticConstraintRejection::RenderFailed,
                error.to_string(),
            )
        })?;
    let template_render = render_semantic_face(
        template,
        template_visual_triangles,
        SimilarityTransform::IDENTITY,
    )
    .map_err(|error| {
        semantic_error(
            receipt.clone(),
            SemanticConstraintRejection::RenderFailed,
            error.to_string(),
        )
    })?;
    let scan_prediction =
        detect_landmarks_rgb_f32(&scan_render.rgb, scan_render.width, scan_render.height).map_err(
            |error| {
                semantic_error(
                    receipt.clone(),
                    SemanticConstraintRejection::InferenceFailed,
                    error.to_string(),
                )
            },
        )?;
    receipt.scan_presence_probability = Some(scan_prediction.presence_probability);
    if scan_prediction.presence_probability < MINIMUM_PRESENCE_PROBABILITY {
        return semantic_rejection(
            receipt,
            SemanticConstraintRejection::FaceNotPresent,
            Some(format!(
                "scan presence {:.4}",
                scan_prediction.presence_probability
            )),
        );
    }
    let template_prediction = detect_landmarks_rgb_f32(
        &template_render.rgb,
        template_render.width,
        template_render.height,
    )
    .map_err(|error| {
        semantic_error(
            receipt.clone(),
            SemanticConstraintRejection::InferenceFailed,
            error.to_string(),
        )
    })?;
    receipt.template_presence_probability = Some(template_prediction.presence_probability);
    if template_prediction.presence_probability < MINIMUM_PRESENCE_PROBABILITY {
        return semantic_rejection(
            receipt,
            SemanticConstraintRejection::FaceNotPresent,
            Some(format!(
                "template presence {:.4}",
                template_prediction.presence_probability
            )),
        );
    }

    let scan_points = semantic_points(&scan_prediction.landmarks);
    let template_points = semantic_points(&template_prediction.landmarks);
    let scan_attachment_triangles = largest_component_triangle_ids(scan);
    let scan_attachments = attach_visible_semantic_points(
        scan,
        &scan_attachment_triangles,
        scan_to_template,
        &scan_render,
        &scan_points,
    );
    let template_attachments = attach_visible_semantic_points(
        template,
        template_skin_triangles,
        SimilarityTransform::IDENTITY,
        &template_render,
        &template_points,
    );
    receipt.scan_surface_hits = scan_attachments.iter().flatten().count();
    receipt.template_surface_hits = template_attachments.iter().flatten().count();

    let mut correspondences = Vec::with_capacity(FACE_OVAL_LANDMARK_COUNT);
    for (landmark, (scan_attachment, template_attachment)) in scan_attachments
        .into_iter()
        .zip(template_attachments)
        .enumerate()
    {
        let (Some(scan_attachment), Some(template_attachment)) =
            (scan_attachment, template_attachment)
        else {
            continue;
        };
        let Some(scan_point) = resolve_attachment(scan, &scan_attachment) else {
            continue;
        };
        let Some(template_point) = resolve_attachment(template, &template_attachment) else {
            continue;
        };
        let distance_cm = (scan_to_template.apply(scan_point) - template_point).norm();
        let normal_dot = attachment_normal(scan, &scan_attachment)
            .zip(attachment_normal(template, &template_attachment))
            .map(|(scan_normal, template_normal)| {
                scan_to_template
                    .rotation
                    .transform_vector(scan_normal)
                    .dot(template_normal)
            })
            .unwrap_or(0.0);
        correspondences.push(AutomaticSurfaceCorrespondence {
            scan: scan_attachment,
            template: template_attachment,
            distance_cm,
            normal_dot,
            region: semantic_region(landmark),
            alignment_share: 0.0,
            fit_share: 0.0,
        });
    }
    receipt.candidate_pairs = correspondences.len();
    let flip = median(
        &correspondences
            .iter()
            .map(|pair| pair.normal_dot)
            .collect::<Vec<_>>(),
    )
    .is_some_and(|value| value < 0.0);
    receipt.normals_were_flipped = flip;
    if flip {
        for pair in &mut correspondences {
            pair.normal_dot *= -1.0;
        }
    }
    let before_filters = correspondences.len();
    correspondences.retain(|pair| {
        pair.distance_cm <= options.maximum_distance_cm
            && pair.normal_dot >= options.minimum_normal_dot
    });
    receipt.distance_normal_rejections = before_filters - correspondences.len();
    correspondences.sort_by(|left, right| {
        left.distance_cm
            .total_cmp(&right.distance_cm)
            .then_with(|| left.template.primitive_id.cmp(&right.template.primitive_id))
            .then_with(|| left.scan.primitive_id.cmp(&right.scan.primitive_id))
    });
    let keep = ((correspondences.len() as f64) * (1.0 - options.trim_fraction)).ceil() as usize;
    receipt.trimmed_outliers = correspondences.len().saturating_sub(keep);
    correspondences.truncate(keep);
    let mut scan_primitives = BTreeSet::new();
    let before_scan_deduplication = correspondences.len();
    correspondences.retain(|pair| {
        pair.scan
            .primitive_id
            .is_none_or(|primitive| scan_primitives.insert(primitive))
    });
    receipt.duplicate_scan_primitives_removed = before_scan_deduplication - correspondences.len();
    let mut template_primitives = BTreeSet::new();
    let before_template_deduplication = correspondences.len();
    correspondences.retain(|pair| {
        pair.template
            .primitive_id
            .is_none_or(|primitive| template_primitives.insert(primitive))
    });
    receipt.duplicate_template_primitives_removed =
        before_template_deduplication - correspondences.len();
    AutomaticSurfaceCorrespondence::normalize_weights(&mut correspondences);
    (
        receipt.regional_constraint_counts,
        receipt.regional_alignment_shares,
        receipt.regional_fit_shares,
    ) = AutomaticSurfaceCorrespondence::regional_receipt(&correspondences);
    receipt.accepted_pairs = correspondences.len();
    receipt.coverage = correspondences.len() as f64 / FACE_OVAL_LANDMARK_COUNT as f64;
    if correspondences.len() < MINIMUM_SEMANTIC_PAIRS {
        return semantic_rejection(
            receipt,
            SemanticConstraintRejection::InsufficientSurfacePairs,
            None,
        );
    }
    receipt.rms_distance_cm = Some(
        (correspondences
            .iter()
            .map(|pair| pair.distance_cm.powi(2))
            .sum::<f64>()
            / correspondences.len() as f64)
            .sqrt(),
    );
    receipt.maximum_distance_cm = correspondences
        .iter()
        .map(|pair| pair.distance_cm)
        .max_by(f64::total_cmp);
    Ok(SemanticSurfaceConstraintResult {
        correspondences,
        receipt,
    })
}

fn semantic_points(
    landmarks: &[vkit_semantic::SemanticLandmark; SEMANTIC_LANDMARK_COUNT],
) -> Vec<[f32; 3]> {
    landmarks[..FACE_OVAL_LANDMARK_COUNT]
        .iter()
        .map(|point| [point.x, point.y, point.z])
        .collect()
}

fn semantic_region(landmark: usize) -> AutomaticConstraintRegion {
    if matches!(
        landmark,
        152 | 148
            | 176
            | 149
            | 150
            | 136
            | 172
            | 58
            | 132
            | 93
            | 234
            | 454
            | 323
            | 361
            | 288
            | 397
            | 365
            | 379
            | 378
            | 400
            | 377
    ) {
        AutomaticConstraintRegion::LowerFaceJawChin
    } else if matches!(
        landmark,
        10 | 338 | 297 | 332 | 284 | 251 | 389 | 356 | 127 | 162 | 21 | 54 | 103 | 67 | 109
    ) {
        AutomaticConstraintRegion::ExteriorEnvelope
    } else if matches!(
        landmark,
        33 | 7
            | 163
            | 144
            | 145
            | 153
            | 154
            | 155
            | 133
            | 173
            | 157
            | 158
            | 159
            | 160
            | 161
            | 246
            | 263
            | 249
            | 390
            | 373
            | 374
            | 380
            | 381
            | 382
            | 362
            | 398
            | 384
            | 385
            | 386
            | 387
            | 388
            | 466
    ) {
        AutomaticConstraintRegion::InteriorEye
    } else if matches!(
        landmark,
        61 | 146
            | 91
            | 181
            | 84
            | 17
            | 314
            | 405
            | 321
            | 375
            | 291
            | 308
            | 324
            | 318
            | 402
            | 317
            | 14
            | 87
            | 178
            | 88
            | 95
            | 185
            | 40
            | 39
            | 37
            | 0
            | 267
            | 269
            | 270
            | 409
            | 78
            | 191
            | 80
            | 81
            | 82
            | 13
            | 312
            | 311
            | 310
            | 415
    ) {
        AutomaticConstraintRegion::InteriorLip
    } else if matches!(
        landmark,
        1 | 2
            | 98
            | 327
            | 168
            | 6
            | 197
            | 195
            | 5
            | 4
            | 45
            | 275
            | 440
            | 344
            | 278
            | 294
            | 64
            | 48
            | 115
            | 220
            | 274
    ) {
        AutomaticConstraintRegion::InteriorNose
    } else {
        AutomaticConstraintRegion::BroadSurface
    }
}

fn resolve_attachment(mesh: &Mesh, attachment: &SurfaceAttachment) -> Option<Vec3> {
    if attachment
        .triangle_vertex_ids
        .iter()
        .any(|index| *index as usize >= mesh.vertices.len())
        || attachment
            .barycentric
            .iter()
            .any(|weight| !weight.is_finite())
    {
        return None;
    }
    Some(
        attachment
            .triangle_vertex_ids
            .iter()
            .zip(attachment.barycentric)
            .fold(Vec3::ZERO, |sum, (index, weight)| {
                sum + Vec3::from(mesh.vertices[*index as usize]) * weight
            }),
    )
}

fn attachment_normal(mesh: &Mesh, attachment: &SurfaceAttachment) -> Option<Vec3> {
    let points = attachment
        .triangle_vertex_ids
        .map(|index| Vec3::from(mesh.vertices[index as usize]));
    let normal = cross(points[1] - points[0], points[2] - points[0]);
    let norm = normal.norm();
    (norm > 1.0e-12 && norm.is_finite()).then_some(normal / norm)
}

fn cross(first: Vec3, second: Vec3) -> Vec3 {
    Vec3::new(
        first.y * second.z - first.z * second.y,
        first.z * second.x - first.x * second.z,
        first.x * second.y - first.y * second.x,
    )
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[middle - 1] + values[middle]) * 0.5)
    } else {
        Some(values[middle])
    }
}

fn semantic_rejection(
    receipt: SemanticSurfaceConstraintReceipt,
    reason: SemanticConstraintRejection,
    detail: Option<String>,
) -> Result<SemanticSurfaceConstraintResult, SemanticSurfaceConstraintError> {
    Err(semantic_error(receipt, reason, detail.unwrap_or_default()))
}

fn semantic_error(
    mut receipt: SemanticSurfaceConstraintReceipt,
    reason: SemanticConstraintRejection,
    detail: String,
) -> SemanticSurfaceConstraintError {
    receipt.rejection_reason = Some(reason);
    receipt.failure_detail = (!detail.is_empty()).then_some(detail);
    SemanticSurfaceConstraintError {
        receipt: Box::new(receipt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_receipt_starts_with_the_fixed_model_contract() {
        let receipt = SemanticSurfaceConstraintReceipt::new();
        assert_eq!(receipt.model_landmark_count, 478);
        assert_eq!(receipt.usable_landmark_count, 468);
        assert_eq!(receipt.accepted_pairs, 0);
        assert_eq!(receipt.rejection_reason, None);
    }

    #[test]
    fn invalid_surface_fails_before_model_initialization() {
        let mesh = Mesh {
            vertices: vec![[0.0, 0.0, 0.0]],
            triangles: Vec::new(),
        };
        let error = build_semantic_surface_correspondences(
            &mesh,
            &mesh,
            &[],
            &[],
            SimilarityTransform::IDENTITY,
        )
        .unwrap_err();
        assert_eq!(
            error.receipt.rejection_reason,
            Some(SemanticConstraintRejection::InvalidSurface)
        );
    }
}
