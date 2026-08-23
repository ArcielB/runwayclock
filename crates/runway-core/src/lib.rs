//! Platform-independent financial domain and no-work runway simulation.
//!
//! This crate knows nothing about SQLite, CSV files, Tauri, or GNOME. Actual
//! ledger data is summarized into a [`RunwayInput`], while forecast rules stay
//! explicit and separately sourced.

mod forecast;
mod money;

pub use forecast::{
    CashFlowDirection, ForecastCadence, ForecastRule, HistoricalBurn, Provenance, RuleContribution,
    RunwayError, RunwayInput, RunwayResult, calculate_runway, format_calendar_duration,
};
pub use money::{CurrencyCode, Money, MoneyError};
