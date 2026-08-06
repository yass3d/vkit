use std::io::Write as _;

use flate2::{Compression, write::DeflateEncoder};

const STORED_EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "zip", "var", "vmb"];

const FIXED_DOS_TIME: (u16, u16) = (0, 0x0021);

#[derive(Default)]
pub(super) struct ZipWriter {
    bytes: Vec<u8>,
    directory: Vec<CentralEntry>,
}

struct CentralEntry {
    name: String,
    method: u16,
    crc: u32,
    compressed: u32,
    uncompressed: u32,
    offset: u32,
}

impl ZipWriter {
    pub(super) fn add(&mut self, name: &str, payload: &[u8]) {
        let stored = name.rsplit_once('.').is_some_and(|(_, extension)| {
            STORED_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        });
        let (method, body) = if stored {
            (0_u16, payload.to_vec())
        } else {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
            let deflated = encoder
                .write_all(payload)
                .and_then(|()| encoder.finish())
                .unwrap_or_default();

            if deflated.is_empty() || deflated.len() >= payload.len() {
                (0, payload.to_vec())
            } else {
                (8, deflated)
            }
        };
        let offset = u32::try_from(self.bytes.len()).unwrap_or(u32::MAX);
        let crc = crc32(payload);
        let compressed = u32::try_from(body.len()).unwrap_or(u32::MAX);
        let uncompressed = u32::try_from(payload.len()).unwrap_or(u32::MAX);

        self.bytes.extend_from_slice(b"PK\x03\x04");
        self.bytes.extend_from_slice(&20_u16.to_le_bytes());
        self.bytes.extend_from_slice(&0_u16.to_le_bytes());
        self.bytes.extend_from_slice(&method.to_le_bytes());
        self.bytes
            .extend_from_slice(&FIXED_DOS_TIME.0.to_le_bytes());
        self.bytes
            .extend_from_slice(&FIXED_DOS_TIME.1.to_le_bytes());
        self.bytes.extend_from_slice(&crc.to_le_bytes());
        self.bytes.extend_from_slice(&compressed.to_le_bytes());
        self.bytes.extend_from_slice(&uncompressed.to_le_bytes());
        self.bytes
            .extend_from_slice(&(name.len() as u16).to_le_bytes());
        self.bytes.extend_from_slice(&0_u16.to_le_bytes());
        self.bytes.extend_from_slice(name.as_bytes());
        self.bytes.extend_from_slice(&body);

        self.directory.push(CentralEntry {
            name: name.to_owned(),
            method,
            crc,
            compressed,
            uncompressed,
            offset,
        });
    }

    pub(super) fn finish(mut self) -> Vec<u8> {
        let directory_offset = u32::try_from(self.bytes.len()).unwrap_or(u32::MAX);
        for entry in &self.directory {
            self.bytes.extend_from_slice(b"PK\x01\x02");
            self.bytes.extend_from_slice(&20_u16.to_le_bytes());
            self.bytes.extend_from_slice(&20_u16.to_le_bytes());
            self.bytes.extend_from_slice(&0_u16.to_le_bytes());
            self.bytes.extend_from_slice(&entry.method.to_le_bytes());
            self.bytes
                .extend_from_slice(&FIXED_DOS_TIME.0.to_le_bytes());
            self.bytes
                .extend_from_slice(&FIXED_DOS_TIME.1.to_le_bytes());
            self.bytes.extend_from_slice(&entry.crc.to_le_bytes());
            self.bytes
                .extend_from_slice(&entry.compressed.to_le_bytes());
            self.bytes
                .extend_from_slice(&entry.uncompressed.to_le_bytes());
            self.bytes
                .extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            self.bytes.extend_from_slice(&0_u16.to_le_bytes());
            self.bytes.extend_from_slice(&0_u16.to_le_bytes());
            self.bytes.extend_from_slice(&0_u16.to_le_bytes());
            self.bytes.extend_from_slice(&0_u16.to_le_bytes());
            self.bytes.extend_from_slice(&0_u32.to_le_bytes());
            self.bytes.extend_from_slice(&entry.offset.to_le_bytes());
            self.bytes.extend_from_slice(entry.name.as_bytes());
        }
        let directory_size = u32::try_from(self.bytes.len()).unwrap_or(u32::MAX) - directory_offset;
        let count = u16::try_from(self.directory.len()).unwrap_or(u16::MAX);
        self.bytes.extend_from_slice(b"PK\x05\x06");
        self.bytes.extend_from_slice(&0_u16.to_le_bytes());
        self.bytes.extend_from_slice(&0_u16.to_le_bytes());
        self.bytes.extend_from_slice(&count.to_le_bytes());
        self.bytes.extend_from_slice(&count.to_le_bytes());
        self.bytes.extend_from_slice(&directory_size.to_le_bytes());
        self.bytes
            .extend_from_slice(&directory_offset.to_le_bytes());
        self.bytes.extend_from_slice(&0_u16.to_le_bytes());
        self.bytes
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn every_entry_survives_the_writer() {
        let payloads: [(&str, Vec<u8>); 4] = [
            ("meta.json", br#"{"schema":1}"#.to_vec()),
            ("morphs/A.txt", vec![b'a'; 4096]),
            ("morphs/B.vmb", (0..=255_u8).cycle().take(3000).collect()),
            ("empty.txt", Vec::new()),
        ];
        let mut archive = ZipWriter::default();
        for (name, bytes) in &payloads {
            archive.add(name, bytes);
        }
        let encoded = archive.finish();

        let entries = read_back(&encoded);
        assert_eq!(entries.len(), payloads.len());
        for ((name, bytes), (expected_name, expected)) in entries.iter().zip(&payloads) {
            assert_eq!(name, expected_name);
            assert_eq!(bytes, expected, "{expected_name}");
        }
    }

    fn read_back(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        use std::io::Read as _;

        let u16_at =
            |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        let u32_at = |offset: usize| {
            u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]) as usize
        };

        let mut entries = Vec::new();
        let mut cursor = 0;
        while bytes[cursor..].starts_with(b"PK\x03\x04") {
            let method = u16_at(cursor + 8);
            let crc = u32_at(cursor + 14) as u32;
            let compressed = u32_at(cursor + 18);
            let uncompressed = u32_at(cursor + 22);
            let name_length = u16_at(cursor + 26);
            let extra_length = u16_at(cursor + 28);
            let name_at = cursor + 30;
            let name = String::from_utf8(bytes[name_at..name_at + name_length].to_vec())
                .expect("entry names are written as UTF-8");
            let body_at = name_at + name_length + extra_length;
            let body = &bytes[body_at..body_at + compressed];
            let payload = match method {
                0 => body.to_vec(),
                8 => {
                    let mut inflated = Vec::new();
                    flate2::read::DeflateDecoder::new(body)
                        .read_to_end(&mut inflated)
                        .expect("the writer's own deflate stream must inflate");
                    inflated
                }
                other => panic!("unexpected compression method {other}"),
            };
            assert_eq!(payload.len(), uncompressed, "{name}: declared length");
            assert_eq!(crc32(&payload), crc, "{name}: declared checksum");
            entries.push((name, payload));
            cursor = body_at + compressed;
        }
        assert!(
            bytes[cursor..].starts_with(b"PK\x01\x02")
                || bytes[cursor..].starts_with(b"PK\x05\x06"),
            "the entries must be followed by the directory that indexes them"
        );
        entries
    }
}
