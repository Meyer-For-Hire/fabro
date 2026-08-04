use serde_json::Value;

/// The reason a parsed JSON value cannot be represented as a TOML scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JsonScalarToTomlError {
    /// JSON null has no TOML scalar representation.
    #[error("JSON null is not a TOML scalar")]
    Null,
    /// JSON arrays are outside the scalar-only conversion contract.
    #[error("JSON arrays are not TOML scalars")]
    Array,
    /// JSON objects are outside the scalar-only conversion contract.
    #[error("JSON objects are not TOML scalars")]
    Object,
    /// The JSON number is not representable by the supported TOML number types.
    #[error("JSON number is outside TOML's supported range")]
    NumberOutOfRange,
}

/// Converts an already-parsed JSON scalar into a TOML value.
///
/// Numbers are converted to a signed integer first and then to a float. As a
/// result, nonnegative integers above `i64::MAX` become TOML floats and may
/// lose integer precision. JSON floats remain TOML floats.
///
/// # Errors
///
/// Returns [`JsonScalarToTomlError`] for JSON null, arrays, objects, or a
/// number that can be represented as neither an `i64` nor an `f64`.
pub fn json_scalar_to_toml_value(value: &Value) -> Result<toml::Value, JsonScalarToTomlError> {
    match value {
        Value::Null => Err(JsonScalarToTomlError::Null),
        Value::Bool(value) => Ok(toml::Value::Boolean(*value)),
        Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                Ok(toml::Value::Integer(integer))
            } else if let Some(float) = value.as_f64() {
                Ok(toml::Value::Float(float))
            } else {
                Err(JsonScalarToTomlError::NumberOutOfRange)
            }
        }
        Value::String(value) => Ok(toml::Value::String(value.clone())),
        Value::Array(_) => Err(JsonScalarToTomlError::Array),
        Value::Object(_) => Err(JsonScalarToTomlError::Object),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{JsonScalarToTomlError, json_scalar_to_toml_value};

    #[test]
    fn converts_strings_to_toml_strings() -> Result<(), JsonScalarToTomlError> {
        for input in ["", "hello", "Grüße 世界"] {
            let json = Value::String(input.to_string());

            assert_eq!(
                json_scalar_to_toml_value(&json)?,
                toml::Value::String(input.to_string()),
                "{input:?}"
            );
        }

        Ok(())
    }

    #[test]
    fn converts_booleans_to_toml_booleans() -> Result<(), JsonScalarToTomlError> {
        for input in [false, true] {
            assert_eq!(
                json_scalar_to_toml_value(&Value::Bool(input))?,
                toml::Value::Boolean(input)
            );
        }

        Ok(())
    }

    #[test]
    fn converts_signed_range_integers_to_toml_integers() -> Result<(), JsonScalarToTomlError> {
        for input in [i64::MIN, -1, 0, i64::MAX] {
            assert_eq!(
                json_scalar_to_toml_value(&Value::from(input))?,
                toml::Value::Integer(input)
            );
        }

        let signed_max_as_u64 =
            u64::try_from(i64::MAX).expect("i64::MAX should be representable as u64");
        assert_eq!(
            json_scalar_to_toml_value(&Value::from(signed_max_as_u64))?,
            toml::Value::Integer(i64::MAX)
        );

        Ok(())
    }

    #[test]
    fn converts_large_unsigned_integers_to_toml_floats() -> Result<(), JsonScalarToTomlError> {
        let signed_max_as_u64 =
            u64::try_from(i64::MAX).expect("i64::MAX should be representable as u64");
        for input in [signed_max_as_u64 + 1, u64::MAX] {
            assert_eq!(
                json_scalar_to_toml_value(&Value::from(input))?,
                toml::Value::Float(input as f64)
            );
        }

        Ok(())
    }

    #[test]
    fn preserves_finite_json_floats_as_toml_floats() -> Result<(), JsonScalarToTomlError> {
        for input in [-0.0, 0.5, 1.0, f64::MAX] {
            let converted = json_scalar_to_toml_value(&Value::from(input))?;
            let toml::Value::Float(output) = converted else {
                panic!("{input:?} should remain a TOML float");
            };

            assert_eq!(output.to_bits(), input.to_bits(), "{input:?}");
        }

        Ok(())
    }

    #[test]
    fn rejects_non_scalar_json_values_with_typed_errors() {
        let cases = [
            (Value::Null, JsonScalarToTomlError::Null),
            (json!(["value"]), JsonScalarToTomlError::Array),
            (json!({ "key": "value" }), JsonScalarToTomlError::Object),
        ];

        for (input, expected) in cases {
            assert_eq!(json_scalar_to_toml_value(&input), Err(expected));
        }
    }

    #[test]
    fn classifies_non_finite_float_values_at_the_json_boundary() {
        for input in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let json = Value::from(input);

            assert_eq!(json, Value::Null);
            assert_eq!(
                json_scalar_to_toml_value(&json),
                Err(JsonScalarToTomlError::Null)
            );
        }
    }
}
