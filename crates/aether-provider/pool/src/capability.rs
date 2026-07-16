#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderPoolCapability {
    PlanTier,
    QuotaReset,
    QuotaRefresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderQuotaServingPolicy {
    ObservationOnly,
    SubscriptionExhaustionOnly,
    ServingProbe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderQuotaHealthTransition {
    Preserve,
    QuotaExhausted,
    Available,
}

impl ProviderQuotaServingPolicy {
    pub const fn subscription_transition(
        self,
        refresh_succeeded: bool,
        exhausted: bool,
        current_exhaustion_is_quota_derived: bool,
    ) -> ProviderQuotaHealthTransition {
        match self {
            Self::ObservationOnly | Self::ServingProbe => ProviderQuotaHealthTransition::Preserve,
            Self::SubscriptionExhaustionOnly if !refresh_succeeded => {
                ProviderQuotaHealthTransition::Preserve
            }
            Self::SubscriptionExhaustionOnly if exhausted => {
                ProviderQuotaHealthTransition::QuotaExhausted
            }
            Self::SubscriptionExhaustionOnly if current_exhaustion_is_quota_derived => {
                ProviderQuotaHealthTransition::Available
            }
            Self::SubscriptionExhaustionOnly => ProviderQuotaHealthTransition::Preserve,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProviderPoolCapabilities {
    pub plan_tier: bool,
    pub quota_reset: bool,
    pub quota_refresh: bool,
}

impl ProviderPoolCapabilities {
    pub fn supports(self, capability: ProviderPoolCapability) -> bool {
        match capability {
            ProviderPoolCapability::PlanTier => self.plan_tier,
            ProviderPoolCapability::QuotaReset => self.quota_reset,
            ProviderPoolCapability::QuotaRefresh => self.quota_refresh,
        }
    }
}
