use crate::{CurrencyCode, Money, MoneyError};
use chrono::{Datelike, Days, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

const DEFAULT_MAX_HORIZON_DAYS: u32 = 36_525;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashFlowDirection {
    Income,
    Expense,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ForecastCadence {
    Once,
    Monthly { day_of_month: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    UserConfirmed,
    DeterministicEstimate,
    ImportedActual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForecastRule {
    pub id: String,
    pub label: String,
    pub direction: CashFlowDirection,
    /// Always a positive magnitude.
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub cadence: ForecastCadence,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub provenance: Provenance,
    /// Integer parts per million. User-confirmed facts use 1,000,000.
    pub confidence_ppm: u32,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalBurn {
    /// Sum of included historical external outflows, after confirmed refunds.
    pub expense_minor: i64,
    /// Inclusive length of the observation window.
    pub observed_days: u32,
    pub observed_from: NaiveDate,
    pub observed_through: NaiveDate,
    pub included_transaction_count: u64,
    pub excluded_transaction_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunwayInput {
    /// Balance as of the end of `as_of`; simulation starts the following day.
    pub as_of: NaiveDate,
    pub liquid_assets: Money,
    pub reserve: Money,
    pub historical_burn: HistoricalBurn,
    pub forecast_rules: Vec<ForecastRule>,
    pub max_horizon_days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleContribution {
    pub rule_id: String,
    pub label: String,
    pub direction: CashFlowDirection,
    pub occurrence_count: u32,
    /// Signed: income is positive, expense is negative.
    pub amount_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunwayResult {
    pub as_of: NaiveDate,
    pub currency: CurrencyCode,
    pub liquid_assets_minor: i64,
    pub reserve_minor: i64,
    pub zero_date: Option<NaiveDate>,
    pub runway_days: Option<u32>,
    pub display_duration: Option<String>,
    pub projected_balance_minor: i64,
    pub historical_expense_applied_minor: i64,
    pub rule_contributions: Vec<RuleContribution>,
    pub last_actual_data: NaiveDate,
    pub horizon_days: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error)]
pub enum RunwayError {
    #[error(transparent)]
    Money(#[from] MoneyError),
    #[error("liquid assets and reserve must use the same currency")]
    CurrencyMismatch,
    #[error("historical expense must not be negative")]
    NegativeHistoricalExpense,
    #[error("historical observation window must contain at least one day")]
    EmptyObservationWindow,
    #[error("historical dates do not match observed_days")]
    InvalidObservationWindow,
    #[error("forecast rule {0:?} has a non-positive amount")]
    InvalidRuleAmount(String),
    #[error("forecast rule {0:?} has an invalid confidence")]
    InvalidConfidence(String),
    #[error("forecast rule {0:?} has an invalid monthly day")]
    InvalidMonthlyDay(String),
    #[error("forecast rule {0:?} ends before it starts")]
    InvalidRuleDates(String),
    #[error("forecast rule {0:?} uses a different currency")]
    RuleCurrencyMismatch(String),
    #[error("date arithmetic overflow")]
    DateOverflow,
    #[error("financial arithmetic overflow")]
    ArithmeticOverflow,
}

pub fn calculate_runway(input: &RunwayInput) -> Result<RunwayResult, RunwayError> {
    validate(input)?;

    let horizon_days = input
        .max_horizon_days
        .unwrap_or(DEFAULT_MAX_HORIZON_DAYS)
        .max(1);
    let mut warnings = Vec::new();
    if input.historical_burn.observed_days < 30 {
        warnings.push(format!(
            "Only {} days of actual spending history are available; the estimate is fragile.",
            input.historical_burn.observed_days
        ));
    }

    if input.liquid_assets.amount_minor <= input.reserve.amount_minor {
        return Ok(RunwayResult {
            as_of: input.as_of,
            currency: input.liquid_assets.currency.clone(),
            liquid_assets_minor: input.liquid_assets.amount_minor,
            reserve_minor: input.reserve.amount_minor,
            zero_date: Some(input.as_of),
            runway_days: Some(0),
            display_duration: Some("0 days".to_owned()),
            projected_balance_minor: input.liquid_assets.amount_minor,
            historical_expense_applied_minor: 0,
            rule_contributions: Vec::new(),
            last_actual_data: input.historical_burn.observed_through,
            horizon_days,
            warnings,
        });
    }

    let mut balance = input.liquid_assets.amount_minor;
    let mut burn_remainder: i128 = 0;
    let burn_numerator = i128::from(input.historical_burn.expense_minor);
    let burn_denominator = i128::from(input.historical_burn.observed_days);
    let mut applied_burn = 0_i64;
    let mut contributions: BTreeMap<String, RuleContribution> = BTreeMap::new();

    for day_number in 1..=horizon_days {
        let date = input
            .as_of
            .checked_add_days(Days::new(u64::from(day_number)))
            .ok_or(RunwayError::DateOverflow)?;

        for rule in &input.forecast_rules {
            if rule_occurs_on(rule, date) {
                let signed_amount = match rule.direction {
                    CashFlowDirection::Income => rule.amount_minor,
                    CashFlowDirection::Expense => -rule.amount_minor,
                };
                balance = balance
                    .checked_add(signed_amount)
                    .ok_or(RunwayError::ArithmeticOverflow)?;
                let contribution =
                    contributions
                        .entry(rule.id.clone())
                        .or_insert_with(|| RuleContribution {
                            rule_id: rule.id.clone(),
                            label: rule.label.clone(),
                            direction: rule.direction,
                            occurrence_count: 0,
                            amount_minor: 0,
                        });
                contribution.occurrence_count = contribution
                    .occurrence_count
                    .checked_add(1)
                    .ok_or(RunwayError::ArithmeticOverflow)?;
                contribution.amount_minor = contribution
                    .amount_minor
                    .checked_add(signed_amount)
                    .ok_or(RunwayError::ArithmeticOverflow)?;
            }
        }

        burn_remainder = burn_remainder
            .checked_add(burn_numerator)
            .ok_or(RunwayError::ArithmeticOverflow)?;
        let todays_burn_i128 = burn_remainder / burn_denominator;
        burn_remainder %= burn_denominator;
        let todays_burn =
            i64::try_from(todays_burn_i128).map_err(|_| RunwayError::ArithmeticOverflow)?;
        balance = balance
            .checked_sub(todays_burn)
            .ok_or(RunwayError::ArithmeticOverflow)?;
        applied_burn = applied_burn
            .checked_add(todays_burn)
            .ok_or(RunwayError::ArithmeticOverflow)?;

        if balance <= input.reserve.amount_minor {
            return Ok(RunwayResult {
                as_of: input.as_of,
                currency: input.liquid_assets.currency.clone(),
                liquid_assets_minor: input.liquid_assets.amount_minor,
                reserve_minor: input.reserve.amount_minor,
                zero_date: Some(date),
                runway_days: Some(day_number),
                display_duration: Some(format_calendar_duration(input.as_of, date)?),
                projected_balance_minor: balance,
                historical_expense_applied_minor: applied_burn,
                rule_contributions: contributions.into_values().collect(),
                last_actual_data: input.historical_burn.observed_through,
                horizon_days,
                warnings,
            });
        }
    }

    warnings.push(format!(
        "The reserve is not reached within the {}-day calculation horizon.",
        horizon_days
    ));
    Ok(RunwayResult {
        as_of: input.as_of,
        currency: input.liquid_assets.currency.clone(),
        liquid_assets_minor: input.liquid_assets.amount_minor,
        reserve_minor: input.reserve.amount_minor,
        zero_date: None,
        runway_days: None,
        display_duration: None,
        projected_balance_minor: balance,
        historical_expense_applied_minor: applied_burn,
        rule_contributions: contributions.into_values().collect(),
        last_actual_data: input.historical_burn.observed_through,
        horizon_days,
        warnings,
    })
}

fn validate(input: &RunwayInput) -> Result<(), RunwayError> {
    if input.liquid_assets.currency != input.reserve.currency {
        return Err(RunwayError::CurrencyMismatch);
    }
    let burn = &input.historical_burn;
    if burn.expense_minor < 0 {
        return Err(RunwayError::NegativeHistoricalExpense);
    }
    if burn.observed_days == 0 {
        return Err(RunwayError::EmptyObservationWindow);
    }
    let actual_days = burn
        .observed_through
        .signed_duration_since(burn.observed_from)
        .num_days()
        .checked_add(1)
        .ok_or(RunwayError::InvalidObservationWindow)?;
    if actual_days != i64::from(burn.observed_days) {
        return Err(RunwayError::InvalidObservationWindow);
    }
    for rule in &input.forecast_rules {
        if rule.amount_minor <= 0 {
            return Err(RunwayError::InvalidRuleAmount(rule.id.clone()));
        }
        if rule.confidence_ppm > 1_000_000 {
            return Err(RunwayError::InvalidConfidence(rule.id.clone()));
        }
        if rule.currency != input.liquid_assets.currency {
            return Err(RunwayError::RuleCurrencyMismatch(rule.id.clone()));
        }
        if rule.ends_on.is_some_and(|end| end < rule.starts_on) {
            return Err(RunwayError::InvalidRuleDates(rule.id.clone()));
        }
        if let ForecastCadence::Monthly { day_of_month } = rule.cadence
            && !(1..=31).contains(&day_of_month)
        {
            return Err(RunwayError::InvalidMonthlyDay(rule.id.clone()));
        }
    }
    Ok(())
}

fn rule_occurs_on(rule: &ForecastRule, date: NaiveDate) -> bool {
    if date < rule.starts_on || rule.ends_on.is_some_and(|end| date > end) {
        return false;
    }
    match rule.cadence {
        ForecastCadence::Once => date == rule.starts_on,
        ForecastCadence::Monthly { day_of_month } => {
            date.day() == day_of_month.min(days_in_month(date.year(), date.month()))
        }
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let next = first
        .checked_add_months(Months::new(1))
        .expect("date in range");
    next.signed_duration_since(first).num_days() as u32
}

/// Express a day span as full calendar months plus remaining days. The start
/// date is the last actual balance date; the end date is the first reserve date.
pub fn format_calendar_duration(start: NaiveDate, end: NaiveDate) -> Result<String, RunwayError> {
    if end <= start {
        return Ok("0 days".to_owned());
    }
    let mut cursor = start;
    let mut months = 0_u32;
    while let Some(next) = cursor.checked_add_months(Months::new(1)) {
        if next > end {
            break;
        }
        cursor = next;
        months = months
            .checked_add(1)
            .ok_or(RunwayError::ArithmeticOverflow)?;
    }
    let days = end.signed_duration_since(cursor).num_days();
    match (months, days) {
        (0, days) => Ok(format!("{days} {}", if days == 1 { "day" } else { "days" })),
        (months, 0) => Ok(format!(
            "{months} {}",
            if months == 1 { "month" } else { "months" }
        )),
        (months, days) => Ok(format!(
            "{months} {} {days} {}",
            if months == 1 { "month" } else { "months" },
            if days == 1 { "day" } else { "days" }
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn try_currency() -> CurrencyCode {
        CurrencyCode::new("TRY").unwrap()
    }

    #[test]
    fn no_work_scenario_counts_scholarship_but_no_assumed_salary() {
        let input = RunwayInput {
            as_of: date("2026-08-20"),
            liquid_assets: Money::new(13_200_000, try_currency()),
            reserve: Money::new(2_000_000, try_currency()),
            historical_burn: HistoricalBurn {
                expense_minor: 1_530_000,
                observed_days: 30,
                observed_from: date("2026-07-22"),
                observed_through: date("2026-08-20"),
                included_transaction_count: 20,
                excluded_transaction_count: 2,
            },
            forecast_rules: vec![ForecastRule {
                id: "scholarship".into(),
                label: "Scholarship".into(),
                direction: CashFlowDirection::Income,
                amount_minor: 650_000,
                currency: try_currency(),
                cadence: ForecastCadence::Monthly { day_of_month: 1 },
                starts_on: date("2026-09-01"),
                ends_on: Some(date("2027-06-30")),
                provenance: Provenance::UserConfirmed,
                confidence_ppm: 1_000_000,
                evidence: vec![],
            }],
            max_horizon_days: None,
        };

        let result = calculate_runway(&input).unwrap();
        assert_eq!(result.zero_date, Some(date("2027-08-03")));
        assert_eq!(result.rule_contributions[0].occurrence_count, 10);
        assert_eq!(result.rule_contributions[0].amount_minor, 6_500_000);
    }

    #[test]
    fn monthly_day_is_clamped_to_last_day_of_month() {
        let rule = ForecastRule {
            id: "rent".into(),
            label: "Rent".into(),
            direction: CashFlowDirection::Expense,
            amount_minor: 100,
            currency: try_currency(),
            cadence: ForecastCadence::Monthly { day_of_month: 31 },
            starts_on: date("2027-01-01"),
            ends_on: None,
            provenance: Provenance::UserConfirmed,
            confidence_ppm: 1_000_000,
            evidence: vec![],
        };
        assert!(rule_occurs_on(&rule, date("2027-02-28")));
        assert!(!rule_occurs_on(&rule, date("2027-02-27")));
    }

    #[test]
    fn calendar_duration_uses_calendar_months() {
        assert_eq!(
            format_calendar_duration(date("2026-08-20"), date("2027-10-02")).unwrap(),
            "13 months 12 days"
        );
    }
}
