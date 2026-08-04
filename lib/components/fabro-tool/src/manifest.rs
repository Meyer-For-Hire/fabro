use fabro_types::JsonScalarToTomlError;
use serde_json::Value;

use super::common::{ToolError, ToolResult};

pub fn json_to_toml_value(key: &str, value: &Value) -> ToolResult<toml::Value> {
    fabro_types::json_scalar_to_toml_value(value)
        .map_err(|error| json_scalar_to_tool_error(key, error))
}

fn json_scalar_to_tool_error(key: &str, error: JsonScalarToTomlError) -> ToolError {
    let message = match error {
        JsonScalarToTomlError::Null => {
            format!("input `{key}` cannot be null; use a string, boolean, or number")
        }
        JsonScalarToTomlError::Array => {
            format!("input `{key}` does not support array values; use a string, boolean, or number")
        }
        JsonScalarToTomlError::Object => format!(
            "input `{key}` does not support object values; use a string, boolean, or number",
        ),
        JsonScalarToTomlError::NumberOutOfRange => {
            format!("input `{key}` contains a number outside TOML's supported range")
        }
    };
    ToolError::message(message)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn json_inputs_convert_scalar_values_to_toml_values() {
        let cases = [
            (json!("hello"), toml::Value::String("hello".to_string())),
            (json!(true), toml::Value::Boolean(true)),
            (json!(42), toml::Value::Integer(42)),
            (json!(0.5), toml::Value::Float(0.5)),
        ];

        for (json, expected) in cases {
            assert_eq!(
                json_to_toml_value("input", &json)
                    .expect("representative JSON scalar should convert"),
                expected
            );
        }
    }

    #[test]
    fn json_input_arrays_and_objects_are_rejected() {
        let array_err = json_to_toml_value("matrix", &json!(["a", 1]))
            .expect_err("JSON arrays should be rejected");
        assert_eq!(
            array_err.as_str(),
            "input `matrix` does not support array values; use a string, boolean, or number",
        );

        let object_err = json_to_toml_value("settings", &json!({ "enabled": true }))
            .expect_err("JSON objects should be rejected");
        assert_eq!(
            object_err.as_str(),
            "input `settings` does not support object values; use a string, boolean, or number",
        );
    }

    #[test]
    fn json_input_null_is_rejected_with_key_name() {
        let err =
            json_to_toml_value("goal", &Value::Null).expect_err("JSON null should be rejected");

        assert_eq!(
            err.as_str(),
            "input `goal` cannot be null; use a string, boolean, or number",
        );
    }

    #[test]
    fn json_input_out_of_range_number_preserves_tool_message() {
        let err = json_scalar_to_tool_error("threshold", JsonScalarToTomlError::NumberOutOfRange);

        assert_eq!(
            err.as_str(),
            "input `threshold` contains a number outside TOML's supported range",
        );
    }
}
