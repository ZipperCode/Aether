use serde::Deserialize;
use serde_json::Number;

use super::super::official_balance::decimal_string;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(super) enum DecimalInput {
    Decimal(String),
    Number(Number),
}

impl DecimalInput {
    pub(super) fn finite_number(&self) -> Option<f64> {
        match self {
            Self::Decimal(value) => value.trim().parse::<f64>().ok(),
            Self::Number(value) => value.as_f64(),
        }
        .filter(|value| value.is_finite())
    }

    pub(super) fn decimal_text(&self) -> Option<String> {
        match self {
            Self::Decimal(value) => decimal_string(&serde_json::Value::String(value.clone())),
            Self::Number(value) => decimal_string(&serde_json::Value::Number(value.clone())),
        }
    }
}
