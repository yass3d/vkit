//! Cross-validation of the hand-written `EXT_meshopt_compression` decoders against an
//! independent implementation.
//!
//! `meshopt-rs` is a separate pure-Rust port of meshoptimizer and is a **dev**-dependency only,
//! so nothing here reaches the shipped binary. Its encoders are the reference: bytes it
//! produces must decode, through our decoder, back to exactly the values that went in. A
//! codec that is 99% correct is more damaging than one that is 0% correct, because a slightly
//! wrong mesh still fits and still looks plausible — this file is the gate that makes such a
//! regression fail loudly instead.

use meshopt_rs::index::IndexEncodingVersion;
use meshopt_rs::index::buffer::{
    decode_index_buffer, encode_index_buffer, encode_index_buffer_bound,
};
use meshopt_rs::index::sequence::{
    decode_index_sequence, encode_index_sequence, encode_index_sequence_bound,
};
use meshopt_rs::vertex::VertexEncodingVersion;
use meshopt_rs::vertex::buffer::{encode_vertex_buffer, encode_vertex_buffer_bound};
use vkit_core::formats::{MeshoptFilter, MeshoptMode, decode_meshopt_buffer_view};

fn pseudo_random(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *seed >> 8
}

fn reference_vertex_stream<const STRIDE: usize>(count: usize, seed: u32) -> Vec<[u8; STRIDE]> {
    let mut seed = seed;
    let mut vertices = vec![[0u8; STRIDE]; count];
    for (element, vertex) in vertices.iter_mut().enumerate() {
        for (lane, byte) in vertex.iter_mut().enumerate() {
            let jitter = (pseudo_random(&mut seed) % 7) as i32 - 3;
            *byte = (element as i32 * 5 + lane as i32 * 31 + jitter) as u8;
        }
    }
    vertices
}

fn encode_with_reference<const STRIDE: usize>(vertices: &[[u8; STRIDE]]) -> Vec<u8> {
    let mut buffer = vec![0u8; encode_vertex_buffer_bound(vertices.len(), STRIDE)];
    let written = encode_vertex_buffer(&mut buffer, vertices, VertexEncodingVersion::V0)
        .expect("reference encoder needs a large enough buffer");
    buffer.truncate(written);
    buffer
}

fn decoded_indices(bytes: &[u8], index_size: usize) -> Vec<u32> {
    bytes
        .chunks_exact(index_size)
        .map(|chunk| {
            if index_size == 2 {
                u32::from(u16::from_le_bytes([chunk[0], chunk[1]]))
            } else {
                u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
            }
        })
        .collect()
}

fn check_attributes<const STRIDE: usize>(count: usize, seed: u32) {
    let vertices = reference_vertex_stream::<STRIDE>(count, seed);
    let encoded = encode_with_reference(&vertices);

    let decoded = decode_meshopt_buffer_view(
        &encoded,
        MeshoptMode::Attributes,
        MeshoptFilter::None,
        count,
        STRIDE,
    )
    .unwrap_or_else(|error| panic!("stride {STRIDE}: {error}"));

    let expected: Vec<u8> = vertices
        .iter()
        .flat_map(|vertex| vertex.iter().copied())
        .collect();
    assert_eq!(decoded.len(), count * STRIDE, "stride {STRIDE} length");
    assert_eq!(decoded, expected, "stride {STRIDE} bytes");
}

#[test]
fn attributes_codec_matches_the_reference_encoder_at_every_stride() {
    check_attributes::<4>(300, 1);
    check_attributes::<8>(300, 2);
    check_attributes::<12>(700, 3);
    check_attributes::<16>(129, 4);
    check_attributes::<32>(64, 5);
    check_attributes::<64>(48, 6);
    check_attributes::<256>(40, 7);
}

#[test]
fn attributes_codec_matches_the_reference_encoder_on_block_boundaries() {
    for count in [1usize, 15, 16, 17, 255, 256, 257] {
        let vertices = reference_vertex_stream::<12>(count, 11);
        let encoded = encode_with_reference(&vertices);
        let decoded = decode_meshopt_buffer_view(
            &encoded,
            MeshoptMode::Attributes,
            MeshoptFilter::None,
            count,
            12,
        )
        .unwrap_or_else(|error| panic!("count {count}: {error}"));

        let expected: Vec<u8> = vertices.iter().flat_map(|v| v.iter().copied()).collect();
        assert_eq!(decoded, expected, "count {count}");
    }
}

fn reference_triangles() -> Vec<u32> {
    let width = 24u32;
    let mut indices = Vec::new();
    for row in 0..width - 1 {
        for column in 0..width - 1 {
            let a = row * width + column;
            indices.extend_from_slice(&[a, a + 1, a + width]);
            indices.extend_from_slice(&[a + 1, a + width + 1, a + width]);
        }
    }
    for i in 0..60u32 {
        indices.extend_from_slice(&[0, i + 1, i + 2]);
    }
    indices
}

fn encode_triangles_with_reference(indices: &[u32], version: IndexEncodingVersion) -> Vec<u8> {
    let vertex_count = indices.iter().copied().max().unwrap_or(0) as usize + 1;
    let mut buffer = vec![0u8; encode_index_buffer_bound(indices.len(), vertex_count)];
    let written = encode_index_buffer(&mut buffer, indices, version)
        .expect("reference encoder needs a large enough buffer");
    buffer.truncate(written);
    buffer
}

/// The TRIANGLES encoder is free to rotate a triangle — it picks whichever of the three
/// rotations lets the leading edge hit the edge FIFO — so the decoded list is not the encoded
/// list index for index. The winding must survive, which is what actually matters to geometry,
/// so this is the identity the test asserts against the source.
fn assert_same_triangles_up_to_rotation(decoded: &[u32], source: &[u32], label: &str) {
    assert_eq!(decoded.len(), source.len(), "{label} length");
    for (triangle, (decoded, source)) in decoded
        .chunks_exact(3)
        .zip(source.chunks_exact(3))
        .enumerate()
    {
        let rotations = [
            [source[0], source[1], source[2]],
            [source[1], source[2], source[0]],
            [source[2], source[0], source[1]],
        ];
        assert!(
            rotations.iter().any(|rotation| rotation == decoded),
            "{label} triangle {triangle}: {decoded:?} is no rotation of {source:?}"
        );
    }
}

fn reference_decode_triangles(encoded: &[u8], count: usize) -> Vec<u32> {
    let mut decoded = vec![0u32; count];
    decode_index_buffer(&mut decoded, encoded)
        .expect("reference decoder must accept its own bytes");
    decoded
}

#[test]
fn triangles_codec_matches_the_reference_decoder_at_both_index_versions() {
    let indices = reference_triangles();

    for (label, version) in [
        ("v0", IndexEncodingVersion::V0),
        ("v1", IndexEncodingVersion::V1),
    ] {
        let encoded = encode_triangles_with_reference(&indices, version);
        let ours = decode_meshopt_buffer_view(
            &encoded,
            MeshoptMode::Triangles,
            MeshoptFilter::None,
            indices.len(),
            4,
        )
        .unwrap_or_else(|error| panic!("{label}: {error}"));
        let ours = decoded_indices(&ours, 4);

        assert_eq!(
            ours,
            reference_decode_triangles(&encoded, indices.len()),
            "{label} disagrees with the reference decoder"
        );
        assert_same_triangles_up_to_rotation(&ours, &indices, label);
    }
}

/// Index codec v1 can restart the fresh-index counter mid-stream, and it encodes strip steps as
/// `last - 1` / `last + 1` instead of spending data bytes. This is the shortest sequence that
/// exercises both, taken from the reference implementation's own regression case; a decoder
/// missing either feature desynchronises here and never resynchronises.
#[test]
fn triangles_codec_survives_a_version_one_restart() {
    let indices: Vec<u32> = vec![0, 1, 2, 2, 1, 3, 0, 1, 2, 2, 1, 5, 2, 1, 4];
    let encoded = encode_triangles_with_reference(&indices, IndexEncodingVersion::V1);

    let ours = decode_meshopt_buffer_view(
        &encoded,
        MeshoptMode::Triangles,
        MeshoptFilter::None,
        indices.len(),
        4,
    )
    .unwrap();
    let ours = decoded_indices(&ours, 4);

    assert_eq!(ours, reference_decode_triangles(&encoded, indices.len()));
    assert_same_triangles_up_to_rotation(&ours, &indices, "v1 restart");
}

#[test]
fn triangles_codec_matches_the_reference_decoder_into_sixteen_bit_indices() {
    let indices: Vec<u32> = (0..100u32)
        .flat_map(|value| [value % 64, (value + 7) % 64, (value + 23) % 64])
        .collect();
    let encoded = encode_triangles_with_reference(&indices, IndexEncodingVersion::V1);
    let ours = decode_meshopt_buffer_view(
        &encoded,
        MeshoptMode::Triangles,
        MeshoptFilter::None,
        indices.len(),
        2,
    )
    .unwrap();

    assert_eq!(
        decoded_indices(&ours, 2),
        reference_decode_triangles(&encoded, indices.len())
    );
}

fn encode_sequence_with_reference(indices: &[u32], version: IndexEncodingVersion) -> Vec<u8> {
    let vertex_count = indices.iter().copied().max().unwrap_or(0) as usize + 1;
    let mut buffer = vec![0u8; encode_index_sequence_bound(indices.len(), vertex_count)];
    let written = encode_index_sequence(&mut buffer, indices, version);
    assert_ne!(written, 0, "reference encoder needs a large enough buffer");
    buffer.truncate(written);
    buffer
}

#[test]
fn index_sequence_codec_matches_the_reference_encoder() {
    let mut seed = 97u32;
    let mut indices: Vec<u32> = Vec::new();
    for step in 0..400u32 {
        indices.push(step);
        indices.push(step.wrapping_add(pseudo_random(&mut seed) % 90_000));
        indices.push(step / 2);
    }

    for (label, version) in [
        ("v0", IndexEncodingVersion::V0),
        ("v1", IndexEncodingVersion::V1),
    ] {
        let encoded = encode_sequence_with_reference(&indices, version);
        let decoded = decode_meshopt_buffer_view(
            &encoded,
            MeshoptMode::Indices,
            MeshoptFilter::None,
            indices.len(),
            4,
        )
        .unwrap_or_else(|error| panic!("{label}: {error}"));
        assert_eq!(decoded_indices(&decoded, 4), indices, "{label}");

        let mut reference = vec![0u32; indices.len()];
        decode_index_sequence(&mut reference, &encoded)
            .expect("reference decoder must accept its own bytes");
        assert_eq!(
            decoded_indices(&decoded, 4),
            reference,
            "{label} disagrees with the reference decoder"
        );
    }
}

#[test]
fn index_sequence_codec_matches_the_reference_encoder_on_a_strip() {
    let indices: Vec<u32> = (0..512u32).collect();
    let encoded = encode_sequence_with_reference(&indices, IndexEncodingVersion::V1);

    let wide = decode_meshopt_buffer_view(
        &encoded,
        MeshoptMode::Indices,
        MeshoptFilter::None,
        indices.len(),
        4,
    )
    .unwrap();
    assert_eq!(decoded_indices(&wide, 4), indices);

    let narrow = decode_meshopt_buffer_view(
        &encoded,
        MeshoptMode::Indices,
        MeshoptFilter::None,
        indices.len(),
        2,
    )
    .unwrap();
    assert_eq!(decoded_indices(&narrow, 2), indices);
}

#[test]
fn every_truncation_of_a_reference_stream_is_refused_rather_than_decoded() {
    let vertices = reference_vertex_stream::<12>(64, 41);
    let attributes = encode_with_reference(&vertices);
    for length in 0..attributes.len() {
        assert!(
            decode_meshopt_buffer_view(
                &attributes[..length],
                MeshoptMode::Attributes,
                MeshoptFilter::None,
                64,
                12
            )
            .is_err(),
            "attributes truncated to {length} bytes must fail"
        );
    }

    let indices: Vec<u32> = (0..90u32).collect();
    let sequence = encode_sequence_with_reference(&indices, IndexEncodingVersion::V1);
    for length in 0..sequence.len() {
        assert!(
            decode_meshopt_buffer_view(
                &sequence[..length],
                MeshoptMode::Indices,
                MeshoptFilter::None,
                indices.len(),
                4
            )
            .is_err(),
            "index sequence truncated to {length} bytes must fail"
        );
    }
}
