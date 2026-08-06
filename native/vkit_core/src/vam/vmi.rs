use std::io::{Read, Write};

use serde::Serialize;
use serde_json::{Map, Number, Value};

use super::{Result, VaMError};

const UTF8_BOM: &[u8] = &[0xef, 0xbb, 0xbf];
const MAX_VMI_BYTES: usize = 16 * 1024 * 1024;
const MAX_JSON_DEPTH: usize = 64;
const MAX_JSON_NODES: usize = 200_000;
const MAX_JSON_STRING_BYTES: usize = 1024 * 1024;
const MAX_FORMULA_COUNT: usize = 100_000;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum VmiNumericScalar {
    Number(Number),
    String(String),
}

impl VmiNumericScalar {
    pub fn value(&self) -> Option<f64> {
        match self {
            Self::Number(number) => number.as_f64().filter(|value| value.is_finite()),
            Self::String(text) => parse_finite_f64(text),
        }
    }

    pub fn count_value(&self) -> Option<u64> {
        match self {
            Self::Number(number) => number
                .as_u64()
                .or_else(|| number.as_f64().and_then(finite_integral_u64)),
            Self::String(text) => text
                .parse::<u64>()
                .ok()
                .or_else(|| parse_finite_f64(text).and_then(finite_integral_u64)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum VmiBooleanScalar {
    Bool(bool),
    String(String),
}

impl VmiBooleanScalar {
    pub fn value(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::String(value) if value.eq_ignore_ascii_case("true") => Some(true),
            Self::String(value) if value.eq_ignore_ascii_case("false") => Some(false),
            Self::String(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VmiDocument {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<VmiNumericScalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<VmiNumericScalar>,
    #[serde(rename = "numDeltas", skip_serializing_if = "Option::is_none")]
    pub num_deltas: Option<VmiNumericScalar>,
    #[serde(rename = "isPoseControl", skip_serializing_if = "Option::is_none")]
    pub is_pose_control: Option<VmiBooleanScalar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formulas: Option<Vec<VmiFormula>>,
    #[serde(flatten)]
    pub unknown_fields: Map<String, Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShapeVmiOptions {
    pub id: String,
    pub display_name: String,
    pub group: String,
    pub region: String,

    pub is_pose_control: bool,
}

impl Default for ShapeVmiOptions {
    fn default() -> Self {
        Self {
            id: "VkitMorph".to_owned(),
            display_name: "Vkit Morph".to_owned(),
            group: "Vkit".to_owned(),
            region: "Morph".to_owned(),
            is_pose_control: false,
        }
    }
}

impl VmiDocument {
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref().or(self.id.as_deref())
    }

    pub fn min_value(&self) -> Option<f64> {
        self.min.as_ref().and_then(VmiNumericScalar::value)
    }

    pub fn max_value(&self) -> Option<f64> {
        self.max.as_ref().and_then(VmiNumericScalar::value)
    }

    pub fn num_deltas_value(&self) -> Option<u64> {
        self.num_deltas
            .as_ref()
            .and_then(VmiNumericScalar::count_value)
    }

    pub fn is_pose_control_value(&self) -> Option<bool> {
        self.is_pose_control
            .as_ref()
            .and_then(VmiBooleanScalar::value)
    }
}

pub fn build_shape_vmi(
    id: impl Into<String>,
    display_name: impl Into<String>,
    delta_count: usize,
) -> VmiDocument {
    build_shape_vmi_with_options(
        &ShapeVmiOptions {
            id: id.into(),
            display_name: display_name.into(),
            ..ShapeVmiOptions::default()
        },
        delta_count,
    )
}

pub fn build_shape_vmi_with_options(options: &ShapeVmiOptions, delta_count: usize) -> VmiDocument {
    VmiDocument {
        id: Some(options.id.clone()),
        display_name: Some(options.display_name.clone()),
        group: Some(options.group.clone()),
        region: Some(options.region.clone()),
        min: Some(VmiNumericScalar::String("-1".to_owned())),
        max: Some(VmiNumericScalar::String("1".to_owned())),
        num_deltas: Some(VmiNumericScalar::String(delta_count.to_string())),
        is_pose_control: Some(VmiBooleanScalar::String(
            options.is_pose_control.to_string(),
        )),
        formulas: Some(Vec::new()),
        unknown_fields: Map::new(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VmiFormula {
    #[serde(rename = "targetType", skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplier: Option<VmiNumericScalar>,
    #[serde(flatten)]
    pub unknown_fields: Map<String, Value>,
}

impl VmiFormula {
    pub fn multiplier_value(&self) -> Option<f64> {
        self.multiplier.as_ref().and_then(VmiNumericScalar::value)
    }
}

pub fn parse_vmi(encoded: &[u8]) -> Result<VmiDocument> {
    if encoded.len() > MAX_VMI_BYTES {
        return Err(invalid(format!(
            "document is {} bytes; the safety limit is {MAX_VMI_BYTES} bytes",
            encoded.len()
        )));
    }

    let encoded = encoded.strip_prefix(UTF8_BOM).unwrap_or(encoded);
    let text = std::str::from_utf8(encoded).map_err(|source| {
        VaMError::UnsupportedEncoding(format!(
            "VMI must be UTF-8 JSON; invalid byte begins at offset {}",
            source.valid_up_to()
        ))
    })?;
    let value: Value = crate::vam::simple_json::parse_document_str(text).map_err(|source| {
        invalid(format!(
            "JSON parse error at line {}, column {}: {source}",
            source.line(),
            source.column()
        ))
    })?;

    let mut budget = JsonBudget::default();
    budget.visit(&value, 1, "root")?;
    parse_document(value)
}

pub fn read_vmi(reader: impl Read) -> Result<VmiDocument> {
    let mut encoded = Vec::new();
    let mut limited = reader.take((MAX_VMI_BYTES + 1) as u64);
    limited
        .read_to_end(&mut encoded)
        .map_err(|source| VaMError::VmiIo {
            operation: "reading",
            source,
        })?;
    parse_vmi(&encoded)
}

pub fn encode_vmi_pretty(document: &VmiDocument) -> Result<Vec<u8>> {
    validate_document(document)?;
    let mut encoded = serde_json::to_vec_pretty(document)
        .map_err(|source| invalid(format!("could not serialize JSON: {source}")))?;
    if encoded.len() >= MAX_VMI_BYTES {
        return Err(invalid(format!(
            "pretty JSON is at least {MAX_VMI_BYTES} bytes; reduce the metadata before writing"
        )));
    }
    encoded.push(b'\n');
    Ok(encoded)
}

pub fn write_vmi_pretty(mut writer: impl Write, document: &VmiDocument) -> Result<()> {
    let encoded = encode_vmi_pretty(document)?;
    writer
        .write_all(&encoded)
        .map_err(|source| VaMError::VmiIo {
            operation: "writing",
            source,
        })
}

fn parse_document(value: Value) -> Result<VmiDocument> {
    let Value::Object(mut fields) = value else {
        return Err(invalid("top-level JSON value must be an object"));
    };

    let id = take_optional_string(&mut fields, "id", "top-level object")?;
    let display_name = take_optional_string(&mut fields, "displayName", "top-level object")?;
    let group = take_optional_string(&mut fields, "group", "top-level object")?;
    let region = take_optional_string(&mut fields, "region", "top-level object")?;
    let min = take_optional_numeric_scalar(&mut fields, "min", "top-level object")?;
    let max = take_optional_numeric_scalar(&mut fields, "max", "top-level object")?;
    let num_deltas = take_optional_numeric_scalar(&mut fields, "numDeltas", "top-level object")?;
    if num_deltas
        .as_ref()
        .is_some_and(|scalar| scalar.count_value().is_none())
    {
        return Err(invalid(
            "top-level object.numDeltas must be a non-negative integer",
        ));
    }
    let is_pose_control =
        take_optional_boolean_scalar(&mut fields, "isPoseControl", "top-level object")?;
    let formulas = match fields.remove("formulas") {
        None => None,
        Some(Value::Array(values)) => {
            if values.len() > MAX_FORMULA_COUNT {
                return Err(invalid(format!(
                    "formulas contains {} records; the safety limit is {MAX_FORMULA_COUNT}",
                    values.len()
                )));
            }
            let mut formulas = Vec::with_capacity(values.len());
            for (index, value) in values.into_iter().enumerate() {
                formulas.push(parse_formula(value, index)?);
            }
            Some(formulas)
        }
        Some(other) => {
            return Err(invalid(format!(
                "top-level field `formulas` must be an array when present, found {}",
                json_kind(&other)
            )));
        }
    };

    Ok(VmiDocument {
        id,
        display_name,
        group,
        region,
        min,
        max,
        num_deltas,
        is_pose_control,
        formulas,
        unknown_fields: fields,
    })
}

fn parse_formula(value: Value, index: usize) -> Result<VmiFormula> {
    let Value::Object(mut fields) = value else {
        return Err(invalid(format!(
            "formulas[{index}] must be an object, found {}",
            json_kind(&value)
        )));
    };
    let context = format!("formulas[{index}]");
    let target_type = take_optional_string(&mut fields, "targetType", &context)?;
    let target = take_optional_string(&mut fields, "target", &context)?;
    let multiplier = take_optional_numeric_scalar(&mut fields, "multiplier", &context)?;

    Ok(VmiFormula {
        target_type,
        target,
        multiplier,
        unknown_fields: fields,
    })
}

fn take_optional_string(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<Option<String>> {
    match fields.remove(name) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(other) => Err(invalid(format!(
            "{context}.{name} must be a string when present, found {}",
            json_kind(&other)
        ))),
    }
}

fn take_optional_numeric_scalar(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<Option<VmiNumericScalar>> {
    let Some(value) = fields.remove(name) else {
        return Ok(None);
    };
    parse_numeric_scalar(value, &format!("{context}.{name}")).map(Some)
}

fn parse_numeric_scalar(value: Value, context: &str) -> Result<VmiNumericScalar> {
    let scalar = match value {
        Value::Number(number) => VmiNumericScalar::Number(number),
        Value::String(text) => VmiNumericScalar::String(text),
        other => {
            return Err(invalid(format!(
                "{context} must be a finite number or numeric string when present, found {}",
                json_kind(&other)
            )));
        }
    };
    if scalar.value().is_none() {
        return Err(invalid(format!(
            "{context} must contain a finite numeric value"
        )));
    }
    Ok(scalar)
}

fn take_optional_boolean_scalar(
    fields: &mut Map<String, Value>,
    name: &str,
    context: &str,
) -> Result<Option<VmiBooleanScalar>> {
    let Some(value) = fields.remove(name) else {
        return Ok(None);
    };
    let scalar = match value {
        Value::Bool(value) => VmiBooleanScalar::Bool(value),
        Value::String(value) => VmiBooleanScalar::String(value),
        other => {
            return Err(invalid(format!(
                "{context}.{name} must be a boolean or `true`/`false` string when present, found {}",
                json_kind(&other)
            )));
        }
    };
    if scalar.value().is_none() {
        return Err(invalid(format!(
            "{context}.{name} string must be `true` or `false` (ASCII case-insensitive)"
        )));
    }
    Ok(Some(scalar))
}

fn parse_finite_f64(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn finite_integral_u64(value: f64) -> Option<u64> {
    if value >= 0.0 && value.fract() == 0.0 && value <= u64::MAX as f64 {
        Some(value as u64)
    } else {
        None
    }
}

fn validate_document(document: &VmiDocument) -> Result<()> {
    reject_reserved_fields(
        &document.unknown_fields,
        &[
            "id",
            "displayName",
            "group",
            "region",
            "min",
            "max",
            "numDeltas",
            "isPoseControl",
            "formulas",
        ],
        "top-level unknown_fields",
    )?;
    if document
        .formulas
        .as_ref()
        .is_some_and(|items| items.len() > MAX_FORMULA_COUNT)
    {
        return Err(invalid(format!(
            "formulas exceeds the safety limit of {MAX_FORMULA_COUNT} records"
        )));
    }

    let mut budget = JsonBudget::default();
    budget.scalar(1, "root")?;
    if let Some(id) = document.id.as_deref() {
        budget.string(id, 2, "id")?;
    }
    if let Some(display_name) = document.display_name.as_deref() {
        budget.string(display_name, 2, "displayName")?;
    }
    if let Some(group) = document.group.as_deref() {
        budget.string(group, 2, "group")?;
    }
    if let Some(region) = document.region.as_deref() {
        budget.string(region, 2, "region")?;
    }
    if let Some(min) = document.min.as_ref() {
        validate_numeric_scalar(min, "min", &mut budget, 2)?;
    }
    if let Some(max) = document.max.as_ref() {
        validate_numeric_scalar(max, "max", &mut budget, 2)?;
    }
    if let Some(num_deltas) = document.num_deltas.as_ref() {
        validate_numeric_scalar(num_deltas, "numDeltas", &mut budget, 2)?;
        if num_deltas.count_value().is_none() {
            return Err(invalid("numDeltas must be a non-negative integer"));
        }
    }
    if let Some(is_pose_control) = document.is_pose_control.as_ref() {
        validate_boolean_scalar(is_pose_control, "isPoseControl", &mut budget, 2)?;
    }
    for (name, value) in &document.unknown_fields {
        budget.key(name, "top-level object")?;
        budget.visit(value, 2, name)?;
    }

    if let Some(formulas) = document.formulas.as_deref() {
        budget.scalar(2, "formulas")?;
        for (index, formula) in formulas.iter().enumerate() {
            let context = format!("formulas[{index}]");
            reject_reserved_fields(
                &formula.unknown_fields,
                &["targetType", "target", "multiplier"],
                &format!("{context}.unknown_fields"),
            )?;
            budget.scalar(3, &context)?;
            if let Some(target_type) = formula.target_type.as_deref() {
                budget.string(target_type, 4, &format!("{context}.targetType"))?;
            }
            if let Some(target) = formula.target.as_deref() {
                budget.string(target, 4, &format!("{context}.target"))?;
            }
            if let Some(multiplier) = formula.multiplier.as_ref() {
                validate_numeric_scalar(
                    multiplier,
                    &format!("{context}.multiplier"),
                    &mut budget,
                    4,
                )?;
            }
            for (name, value) in &formula.unknown_fields {
                budget.key(name, &context)?;
                budget.visit(value, 4, &format!("{context}.{name}"))?;
            }
        }
    }
    Ok(())
}

fn validate_numeric_scalar(
    scalar: &VmiNumericScalar,
    context: &str,
    budget: &mut JsonBudget,
    depth: usize,
) -> Result<()> {
    if scalar.value().is_none() {
        return Err(invalid(format!(
            "{context} must contain a finite numeric value"
        )));
    }
    match scalar {
        VmiNumericScalar::Number(_) => budget.scalar(depth, context),
        VmiNumericScalar::String(value) => budget.string(value, depth, context),
    }
}

fn validate_boolean_scalar(
    scalar: &VmiBooleanScalar,
    context: &str,
    budget: &mut JsonBudget,
    depth: usize,
) -> Result<()> {
    if scalar.value().is_none() {
        return Err(invalid(format!(
            "{context} string must be `true` or `false` (ASCII case-insensitive)"
        )));
    }
    match scalar {
        VmiBooleanScalar::Bool(_) => budget.scalar(depth, context),
        VmiBooleanScalar::String(value) => budget.string(value, depth, context),
    }
}

fn reject_reserved_fields(
    fields: &Map<String, Value>,
    reserved: &[&str],
    context: &str,
) -> Result<()> {
    if let Some(name) = reserved.iter().find(|name| fields.contains_key(**name)) {
        return Err(invalid(format!(
            "{context} must not duplicate supported field `{name}`"
        )));
    }
    Ok(())
}

#[derive(Default)]
struct JsonBudget {
    nodes: usize,
}

impl JsonBudget {
    fn visit(&mut self, value: &Value, depth: usize, context: &str) -> Result<()> {
        self.scalar(depth, context)?;
        match value {
            Value::String(value) => self.check_string(value, context),
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    self.visit(value, depth + 1, &format!("{context}[{index}]"))?;
                }
                Ok(())
            }
            Value::Object(fields) => {
                for (name, value) in fields {
                    self.key(name, context)?;
                    self.visit(value, depth + 1, &format!("{context}.{name}"))?;
                }
                Ok(())
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
        }
    }

    fn string(&mut self, value: &str, depth: usize, context: &str) -> Result<()> {
        self.scalar(depth, context)?;
        self.check_string(value, context)
    }

    fn key(&self, value: &str, context: &str) -> Result<()> {
        self.check_string(value, &format!("object key in {context}"))
    }

    fn check_string(&self, value: &str, context: &str) -> Result<()> {
        if value.len() > MAX_JSON_STRING_BYTES {
            return Err(invalid(format!(
                "{context} is {} UTF-8 bytes; the per-string limit is {MAX_JSON_STRING_BYTES}",
                value.len()
            )));
        }
        Ok(())
    }

    fn scalar(&mut self, depth: usize, context: &str) -> Result<()> {
        if depth > MAX_JSON_DEPTH {
            return Err(invalid(format!(
                "{context} exceeds the maximum JSON nesting depth of {MAX_JSON_DEPTH}"
            )));
        }
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or_else(|| invalid("JSON node count overflow while validating the VMI document"))?;
        if self.nodes > MAX_JSON_NODES {
            return Err(invalid(format!(
                "document exceeds the safety limit of {MAX_JSON_NODES} JSON values"
            )));
        }
        Ok(())
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn invalid(message: impl Into<String>) -> VaMError {
    VaMError::InvalidVmi(message.into())
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn accepts_bom_and_uses_id_as_display_name_fallback() {
        let mut encoded = UTF8_BOM.to_vec();
        encoded.extend_from_slice(br#"{"id":"creator:morph","vendor":true}"#);

        let document = parse_vmi(&encoded).unwrap();
        assert_eq!(document.display_name(), Some("creator:morph"));
        assert_eq!(
            document.unknown_fields.get("vendor"),
            Some(&Value::Bool(true))
        );
    }

    #[test]
    fn parses_supported_formula_fields_and_preserves_unknown_json() {
        let encoded = br#"{
            "id":"sample",
            "displayName":"Sample Morph",
            "vendor":{"revision":3,"flags":[true,null]},
            "formulas":[{
                "targetType":"Morph",
                "target":"head/eyes",
                "multiplier":1.25,
                "futureField":{"mode":"curve"}
            }]
        }"#;
        let first = parse_vmi(encoded).unwrap();
        assert_eq!(first.display_name(), Some("Sample Morph"));
        let formula = &first.formulas.as_ref().unwrap()[0];
        assert_eq!(formula.target_type.as_deref(), Some("Morph"));
        assert_eq!(formula.target.as_deref(), Some("head/eyes"));
        assert_eq!(formula.multiplier_value(), Some(1.25));
        assert!(formula.unknown_fields.contains_key("futureField"));

        let pretty = encode_vmi_pretty(&first).unwrap();
        assert!(pretty.ends_with(b"\n"));
        let second = parse_vmi(&pretty).unwrap();
        assert_eq!(second, first);
        assert_eq!(pretty, encode_vmi_pretty(&second).unwrap());
    }

    #[test]
    fn accepts_real_schema_scalar_representations_losslessly() {
        let encoded = br#"{
            "id":"custom-shape",
            "displayName":"Custom Shape",
            "group":"Morphs/Morph Loader",
            "region":"Morphs/Morph Loader",
            "min":"-1",
            "max":2,
            "numDeltas":"21556",
            "isPoseControl":"FaLsE",
            "formulas":[{
                "targetType":"BoneCenterX",
                "target":"lEye",
                "multiplier":"-9.299755E-05"
            }],
            "vendor":{"revision":7}
        }"#;

        let first = parse_vmi(encoded).unwrap();
        assert_eq!(first.group.as_deref(), Some("Morphs/Morph Loader"));
        assert_eq!(first.region.as_deref(), Some("Morphs/Morph Loader"));
        assert_eq!(first.min_value(), Some(-1.0));
        assert_eq!(first.max_value(), Some(2.0));
        assert_eq!(first.num_deltas_value(), Some(21_556));
        assert_eq!(first.is_pose_control_value(), Some(false));
        assert!(matches!(
            first.min.as_ref(),
            Some(VmiNumericScalar::String(value)) if value == "-1"
        ));
        assert!(matches!(
            first.max.as_ref(),
            Some(VmiNumericScalar::Number(_))
        ));
        assert!(matches!(
            first.is_pose_control.as_ref(),
            Some(VmiBooleanScalar::String(value)) if value == "FaLsE"
        ));
        let formula = &first.formulas.as_ref().unwrap()[0];
        assert_eq!(formula.multiplier_value(), Some(-9.299755E-05));
        assert!(matches!(
            formula.multiplier.as_ref(),
            Some(VmiNumericScalar::String(value)) if value == "-9.299755E-05"
        ));

        let pretty = encode_vmi_pretty(&first).unwrap();
        let second = parse_vmi(&pretty).unwrap();
        assert_eq!(second, first);
        assert_eq!(pretty, encode_vmi_pretty(&second).unwrap());

        let boolean = parse_vmi(br#"{"isPoseControl":true}"#).unwrap();
        assert_eq!(boolean.is_pose_control_value(), Some(true));
        assert!(matches!(
            boolean.is_pose_control,
            Some(VmiBooleanScalar::Bool(true))
        ));
    }

    #[test]
    fn shape_builder_emits_deterministic_observed_metadata() {
        let document = build_shape_vmi("shape-id", "Shape Name", 73);
        assert_eq!(document.group.as_deref(), Some("Vkit"));
        assert_eq!(document.region.as_deref(), Some("Morph"));
        assert_eq!(document.min_value(), Some(-1.0));
        assert_eq!(document.max_value(), Some(1.0));
        assert_eq!(document.num_deltas_value(), Some(73));
        assert_eq!(document.is_pose_control_value(), Some(false));
        assert_eq!(document.formulas.as_deref(), Some([].as_slice()));

        let first = encode_vmi_pretty(&document).unwrap();
        let second = encode_vmi_pretty(&document).unwrap();
        assert_eq!(first, second);
        let value: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(value["min"], Value::String("-1".to_owned()));
        assert_eq!(value["max"], Value::String("1".to_owned()));
        assert_eq!(value["numDeltas"], Value::String("73".to_owned()));
        assert_eq!(value["isPoseControl"], Value::String("false".to_owned()));
        assert_eq!(value["formulas"], Value::Array(Vec::new()));
    }

    #[test]
    fn shape_builder_options_preserve_catalog_metadata_without_inventing_formulas() {
        let document = build_shape_vmi_with_options(
            &ShapeVmiOptions {
                id: "artist-smile".to_owned(),
                display_name: "Artist Smile".to_owned(),
                group: "Expressions".to_owned(),
                region: "Face/Mouth".to_owned(),
                is_pose_control: true,
            },
            19,
        );

        assert_eq!(document.id.as_deref(), Some("artist-smile"));
        assert_eq!(document.display_name.as_deref(), Some("Artist Smile"));
        assert_eq!(document.group.as_deref(), Some("Expressions"));
        assert_eq!(document.region.as_deref(), Some("Face/Mouth"));
        assert_eq!(document.num_deltas_value(), Some(19));
        assert_eq!(document.is_pose_control_value(), Some(true));
        assert!(document.formulas.as_ref().is_some_and(Vec::is_empty));

        let encoded = encode_vmi_pretty(&document).unwrap();
        let reparsed = parse_vmi(&encoded).unwrap();
        assert_eq!(reparsed, document);
    }

    #[test]
    fn requires_object_root_and_object_formula_records() {
        let root_error = parse_vmi(br#"[]"#).unwrap_err().to_string();
        assert!(root_error.contains("top-level JSON value must be an object"));

        let list_error = parse_vmi(br#"{"formulas":{}}"#).unwrap_err().to_string();
        assert!(list_error.contains("`formulas` must be an array"));

        let record_error = parse_vmi(br#"{"formulas":["bad"]}"#)
            .unwrap_err()
            .to_string();
        assert!(record_error.contains("formulas[0] must be an object"));
    }

    #[test]
    fn rejects_wrong_supported_field_types() {
        let display_error = parse_vmi(br#"{"displayName":7}"#).unwrap_err().to_string();
        assert!(display_error.contains("displayName must be a string"));

        let multiplier_error = parse_vmi(br#"{"formulas":[{"multiplier":"many"}]}"#)
            .unwrap_err()
            .to_string();
        assert!(multiplier_error.contains("multiplier must contain a finite numeric value"));
    }

    #[test]
    fn rejects_malformed_real_schema_scalars_with_field_diagnostics() {
        for (encoded, field) in [
            (br#"{"min":"NaN"}"#.as_slice(), "min"),
            (br#"{"max":"1e999"}"#.as_slice(), "max"),
            (br#"{"numDeltas":"1.5"}"#.as_slice(), "numDeltas"),
            (br#"{"isPoseControl":"yes"}"#.as_slice(), "isPoseControl"),
            (
                br#"{"formulas":[{"multiplier":false}]}"#.as_slice(),
                "formulas[0].multiplier",
            ),
        ] {
            let error = parse_vmi(encoded).unwrap_err().to_string();
            assert!(error.contains(field), "missing `{field}` in `{error}`");
        }

        let mut malformed = build_shape_vmi("id", "name", 1);
        malformed.min = Some(VmiNumericScalar::String("infinite".to_owned()));
        let error = encode_vmi_pretty(&malformed).unwrap_err().to_string();
        assert!(error.contains("min must contain a finite numeric value"));
    }

    #[test]
    fn enforces_depth_and_reader_size_limits() {
        let mut nested = String::from("{\"future\":");
        nested.push_str(&"[".repeat(MAX_JSON_DEPTH));
        nested.push_str("null");
        nested.push_str(&"]".repeat(MAX_JSON_DEPTH));
        nested.push('}');
        let depth_error = parse_vmi(nested.as_bytes()).unwrap_err().to_string();
        assert!(depth_error.contains("maximum JSON nesting depth"));

        let oversized = vec![b' '; MAX_VMI_BYTES + 1];
        let size_error = read_vmi(oversized.as_slice()).unwrap_err().to_string();
        assert!(size_error.contains("safety limit"));
    }

    #[test]
    fn surfaces_read_and_write_io_failures() {
        struct BrokenReader;
        impl Read for BrokenReader {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("reader broke"))
            }
        }

        let error = read_vmi(BrokenReader).unwrap_err().to_string();
        assert!(error.contains("VMI I/O error while reading"));
    }
}
