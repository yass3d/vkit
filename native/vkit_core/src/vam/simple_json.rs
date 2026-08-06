use serde::Deserialize;
use serde_json::Value;

pub(crate) fn parse_document(bytes: &[u8]) -> serde_json::Result<Value> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);

    Value::deserialize(&mut deserializer)
}

pub(crate) fn parse_document_str(text: &str) -> serde_json::Result<Value> {
    let text = text.trim_start_matches('\u{feff}');
    let mut deserializer = serde_json::Deserializer::from_str(text);
    Value::deserialize(&mut deserializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_credits_after_the_root_object_are_ignored() {
        let bytes =
            b"{\"storables\":[]}\r\nThanks:\"everyone\"+\"hair_version: haaappy_H020.207\"\r\n";
        let value = parse_document(bytes).unwrap();
        assert!(value.get("storables").is_some());
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_document() {
        let bytes = b"\xef\xbb\xbf{\"id\":\"geometry\"}";
        let value = parse_document(bytes).unwrap();
        assert_eq!(value.get("id").and_then(Value::as_str), Some("geometry"));
    }

    #[test]
    fn a_truncated_document_still_fails() {
        assert!(parse_document(b"{\"storables\":[").is_err());
        assert!(parse_document_str("{\"storables\":[").is_err());
    }
}
