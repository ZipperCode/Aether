use serde::Deserialize;
use serde_json::Number;

use crate::quota_snapshot::ProviderQuotaValue;

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

    pub(super) fn quota_value(&self) -> Option<ProviderQuotaValue> {
        self.finite_number()?;
        match self {
            Self::Decimal(value) => Some(ProviderQuotaValue::Decimal(value.trim().to_owned())),
            Self::Number(value) => Some(ProviderQuotaValue::Number(value.clone())),
        }
    }

    pub(super) fn decimal_text(&self) -> Option<String> {
        self.finite_number()?;
        match self {
            Self::Decimal(value) => Some(value.trim().to_owned()),
            Self::Number(value) => Some(value.to_string()),
        }
    }
}
