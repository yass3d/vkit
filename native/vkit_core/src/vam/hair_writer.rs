use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct HairVabDoc {
    pub provider_name: String,
    pub segments: usize,
    pub segment_length_cm: f32,
    pub scalp_vertex_count: usize,
    pub strands_by_scalp_cm: BTreeMap<u32, Vec<[f32; 3]>>,
    pub indices: Vec<u32>,
    pub rigidities: Option<Vec<f32>>,
    pub style_joints: crate::vam::hair_joints::StyleJointGroups,
}

fn write_varint_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    let mut length = bytes.len();
    loop {
        let byte = (length & 0x7f) as u8;
        length >>= 7;
        out.push(if length > 0 { byte | 0x80 } else { byte });
        if length == 0 {
            break;
        }
    }
    out.extend_from_slice(bytes);
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn encode_hair_vab(doc: &HairVabDoc) -> Result<Vec<u8>, String> {
    if doc.segments < 2 {
        return Err("hair segments must be 2 or greater".to_owned());
    }
    if doc.segment_length_cm <= 0.0 {
        return Err("segment length must be positive".to_owned());
    }
    let mut total_points = 0usize;
    for (scalp_index, points) in &doc.strands_by_scalp_cm {
        if *scalp_index as usize >= doc.scalp_vertex_count {
            return Err(format!(
                "strand root {scalp_index} is outside the provider's {} vertices",
                doc.scalp_vertex_count
            ));
        }
        if points.len() != doc.segments {
            return Err(format!(
                "strand {scalp_index} has {} points, the part contract is {} — ragged strands crash VaM",
                points.len(),
                doc.segments
            ));
        }
        total_points += points.len();
    }
    let active = doc.strands_by_scalp_cm.len();
    if !doc.indices.len().is_multiple_of(3) {
        return Err("render indices must form whole triangles".to_owned());
    }
    if doc.indices.iter().any(|index| *index as usize >= active) {
        return Err("render index beyond the active strand count".to_owned());
    }
    if let Some(rigidities) = &doc.rigidities
        && rigidities.len() != total_points
    {
        return Err("rigidity count must match the flattened point count".to_owned());
    }

    let schema = if doc.rigidities.is_some() {
        "1.1"
    } else {
        "1.0"
    };
    let mut out = Vec::with_capacity(64 + total_points * 12 * 2);

    write_varint_string(&mut out, "DynamicStore");
    write_varint_string(&mut out, "1.0");
    out.push(1);
    write_varint_string(&mut out, "RuntimeHairGeometryCreator");
    write_varint_string(&mut out, schema);
    write_varint_string(&mut out, &doc.provider_name);
    write_i32(&mut out, doc.segments as i32);
    write_f32(&mut out, doc.segment_length_cm / 100.0);
    write_varint_string(&mut out, "");

    write_i32(&mut out, doc.scalp_vertex_count as i32);
    for index in 0..doc.scalp_vertex_count as u32 {
        out.push(u8::from(!doc.strands_by_scalp_cm.contains_key(&index)));
    }

    write_i32(&mut out, doc.scalp_vertex_count as i32);
    for index in 0..doc.scalp_vertex_count as u32 {
        write_i32(&mut out, index as i32);
        match doc.strands_by_scalp_cm.get(&index) {
            Some(points) => {
                write_i32(&mut out, points.len() as i32);
                for point in points {
                    write_f32(&mut out, point[0] / 100.0);
                    write_f32(&mut out, point[1] / 100.0);
                    write_f32(&mut out, point[2] / 100.0);
                }
            }
            None => write_i32(&mut out, 0),
        }
    }

    write_i32(&mut out, doc.indices.len() as i32);
    for index in &doc.indices {
        write_i32(&mut out, *index as i32);
    }

    write_i32(&mut out, total_points as i32);
    for points in doc.strands_by_scalp_cm.values() {
        for point in points {
            write_f32(&mut out, point[0] / 100.0);
            write_f32(&mut out, point[1] / 100.0);
            write_f32(&mut out, point[2] / 100.0);
        }
    }

    if let Some(rigidities) = &doc.rigidities {
        write_i32(&mut out, rigidities.len() as i32);
        for value in rigidities {
            write_f32(&mut out, *value);
        }
    }

    write_i32(&mut out, active as i32);
    for scalp_index in doc.strands_by_scalp_cm.keys() {
        write_i32(&mut out, *scalp_index as i32);
    }

    if doc.style_joints.is_empty() {
        write_i32(&mut out, 1);
        write_i32(&mut out, 0);
    } else {
        write_i32(&mut out, doc.style_joints.len() as i32);
        for group in &doc.style_joints {
            write_i32(&mut out, group.len() as i32);
            for joint in group {
                if joint.a as usize >= total_points || joint.b as usize >= total_points {
                    return Err("style joint points beyond the flattened points".to_owned());
                }
                write_f32(&mut out, joint.a as f32);
                write_f32(&mut out, joint.b as f32);
                write_f32(&mut out, joint.distance_m);
                write_f32(&mut out, joint.closeness);
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vam::parse_hair_vab;

    fn sample_doc() -> HairVabDoc {
        let mut strands = BTreeMap::new();
        for (index, x) in [(0u32, 0.0f32), (2, 4.0), (3, 8.0)] {
            strands.insert(
                index,
                vec![
                    [x, 10.0, 0.0],
                    [x, 11.0, 0.2],
                    [x, 12.0, 0.5],
                    [x, 13.0, 0.9],
                ],
            );
        }
        HairVabDoc {
            provider_name: "UdaneScalp".to_owned(),
            segments: 4,
            segment_length_cm: 1.0,
            scalp_vertex_count: 6,
            strands_by_scalp_cm: strands,
            indices: vec![0, 1, 2],
            rigidities: None,
            style_joints: Vec::new(),
        }
    }

    #[test]
    fn encoded_vab_round_trips_through_the_parser() {
        let doc = sample_doc();
        let bytes = encode_hair_vab(&doc).expect("encode");
        let parsed = parse_hair_vab(&bytes, "round-trip").expect("parse");

        assert_eq!(parsed.provider_name, doc.provider_name);
        assert_eq!(parsed.segments, doc.segments);
        assert_eq!(parsed.scalp_vertex_count, doc.scalp_vertex_count);
        assert_eq!(parsed.root_map, vec![0, 2, 3]);
        assert_eq!(parsed.guide_triangles, vec![[0, 1, 2]]);
        assert_eq!(parsed.guides.len(), 3);
        for (guide, (scalp_index, points)) in
            parsed.guides.iter().zip(doc.strands_by_scalp_cm.iter())
        {
            assert_eq!(guide.scalp_index, *scalp_index);
            assert_eq!(guide.points_cm.len(), points.len());
            for (parsed_point, original) in guide.points_cm.iter().zip(points) {
                for axis in 0..3 {
                    assert!(
                        (parsed_point[axis] - original[axis]).abs() < 1e-3,
                        "point drifted: {parsed_point:?} vs {original:?}"
                    );
                }
            }
        }
        assert!((parsed.segment_length_cm - doc.segment_length_cm).abs() < 1e-4);
    }

    #[test]
    fn ragged_strands_are_refused() {
        let mut doc = sample_doc();
        doc.strands_by_scalp_cm.get_mut(&0).unwrap().pop();
        assert!(encode_hair_vab(&doc).unwrap_err().contains("ragged"));
    }

    #[test]
    fn out_of_range_roots_and_indices_are_refused() {
        let mut doc = sample_doc();
        doc.strands_by_scalp_cm
            .insert(99, doc.strands_by_scalp_cm[&0].clone());
        assert!(encode_hair_vab(&doc).is_err());

        let mut doc = sample_doc();
        doc.indices = vec![0, 1, 3];
        assert!(encode_hair_vab(&doc).is_err());
    }
}
#[cfg(test)]
mod joint_tests {
    use super::*;
    use crate::vam::hair_joints::build_style_joints;
    use crate::vam::parse_hair_vab;
    use std::collections::BTreeMap;

    #[test]
    fn style_joints_survive_the_round_trip() {
        let mut strands = BTreeMap::new();
        for (index, x) in [(0u32, 0.0f32), (1, 0.5)] {
            strands.insert(
                index,
                vec![
                    [x, 10.0, 0.0],
                    [x, 11.0, 0.0],
                    [x, 12.0, 0.0],
                    [x, 13.0, 0.0],
                ],
            );
        }
        let flattened: Vec<&[[f32; 3]]> =
            strands.values().map(|points| points.as_slice()).collect();
        let style_joints = build_style_joints(&flattened, 1.0);
        let joint_count: usize = style_joints.iter().map(Vec::len).sum();
        assert!(joint_count > 0, "the fixture must produce joints");
        let doc = HairVabDoc {
            provider_name: "TestScalp".to_owned(),
            segments: 4,
            segment_length_cm: 1.0,
            scalp_vertex_count: 4,
            strands_by_scalp_cm: strands,
            indices: vec![0, 1, 1],
            rigidities: None,
            style_joints,
        };
        let bytes = encode_hair_vab(&doc).expect("encodes");
        let parsed = parse_hair_vab(&bytes, "round-trip").expect("parses");
        assert_eq!(
            parsed.nearby_joints.len(),
            joint_count,
            "every written joint must come back",
        );
        for joint in &parsed.nearby_joints {
            assert_ne!(joint.a[0], joint.b[0], "joints tie different strands");
            assert!((0.0..=1.0).contains(&joint.elasticity));
        }
    }
}

#[cfg(test)]
mod cap_only_items {
    use super::*;

    #[test]
    fn an_item_that_grows_nothing_still_writes() {
        let doc = HairVabDoc {
            provider_name: "UdaneScalp".to_owned(),
            segments: 16,
            segment_length_cm: 1.0,
            scalp_vertex_count: 8,
            strands_by_scalp_cm: std::collections::BTreeMap::new(),
            indices: Vec::new(),
            rigidities: None,
            style_joints: crate::vam::hair_joints::StyleJointGroups::default(),
        };
        let bytes = encode_hair_vab(&doc)
            .expect("VaM ships hairs that are nothing but a cap, so we must write one");
        let read = crate::vam::parse_hair_vab(&bytes, "cap-only").expect("and read it back");
        assert!(read.guides.is_empty());
        assert_eq!(read.provider_name, "UdaneScalp");
        assert_eq!(read.scalp_vertex_count, 8);
    }
}
