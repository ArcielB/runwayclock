use chrono::NaiveDate;
use runway_core::{
    CashFlowDirection, CurrencyCode, ForecastCadence, ForecastRule, HistoricalBurn, Money,
    Provenance, RunwayInput, RunwayResult,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Money(#[from] runway_core::MoneyError),
    #[error("scenario {0:?} does not exist")]
    ScenarioNotFound(String),
    #[error("import profile {0:?} does not exist")]
    ProfileNotFound(String),
    #[error("no actual transactions exist for {0}")]
    NoActualTransactions(String),
    #[error("no account balance is available for {currency} as of {as_of}; set explicit assets")]
    NoAccountBalance { currency: String, as_of: NaiveDate },
    #[error(
        "cannot move as-of date beyond actual data ({last_actual}) without an explicit asset fact for that date"
    )]
    StaleBalanceOverride {
        requested: NaiveDate,
        last_actual: NaiveDate,
    },
    #[error("invalid date {value:?} stored in {field}")]
    InvalidStoredDate { field: &'static str, value: String },
    #[error("invalid database value: {0}")]
    InvalidValue(String),
    #[error("database schema version {0} is newer than this RunwayClock build")]
    UnsupportedSchema(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    pub id: i64,
    pub name: String,
    pub currency: CurrencyCode,
    pub reserve_minor: i64,
    pub explicit_assets_minor: Option<i64>,
    pub assets_as_of: Option<NaiveDate>,
    pub max_horizon_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewForecastRule {
    pub label: String,
    pub direction: CashFlowDirection,
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub cadence: ForecastCadence,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransactionListItem {
    pub id: i64,
    pub booked_on: NaiveDate,
    pub account: String,
    pub description: String,
    pub amount_minor: i64,
    pub currency: String,
    pub balance_after_minor: Option<i64>,
    pub interpretation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountSummary {
    pub external_key: String,
    pub display_name: String,
    pub currency: String,
    pub transaction_count: u64,
    pub last_actual_data: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppSummary {
    pub transaction_count: u64,
    pub last_actual_data: Option<NaiveDate>,
    pub accounts: Vec<AccountSummary>,
    pub scenarios: Vec<String>,
    pub unresolved_outflow_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SavedImportProfile {
    pub name: String,
    pub config_toml: String,
    pub account_key: Option<String>,
    pub account_name: Option<String>,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ForecastRuleListItem {
    pub id: i64,
    pub label: String,
    pub direction: String,
    pub amount_minor: i64,
    pub currency: String,
    pub cadence: String,
    pub day_of_month: Option<u32>,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub source: String,
    pub evidence: Vec<String>,
}

pub fn open(path: impl AsRef<Path>) -> Result<Connection, DbError> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    migrate(&connection)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    Ok(connection)
}

pub fn open_in_memory() -> Result<Connection, DbError> {
    let connection = Connection::open_in_memory()?;
    migrate(&connection)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    Ok(connection)
}

pub fn migrate(connection: &Connection) -> Result<(), DbError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        0 => {
            connection.execute_batch(include_str!("schema.sql"))?;
            connection.pragma_update(None, "user_version", 2)?;
        }
        1 => connection.execute_batch(include_str!("migration_v2.sql"))?,
        2 => {}
        other => return Err(DbError::UnsupportedSchema(other)),
    }
    Ok(())
}

pub fn save_import_profile(
    connection: &Connection,
    name: &str,
    config_toml: &str,
) -> Result<i64, DbError> {
    connection.execute(
        "INSERT INTO import_profiles(name, config_toml)
         VALUES (?1, ?2)
         ON CONFLICT(name) DO UPDATE SET
            config_toml = excluded.config_toml,
            updated_at = CURRENT_TIMESTAMP",
        params![name, config_toml],
    )?;
    Ok(connection.query_row(
        "SELECT id FROM import_profiles WHERE name = ?1",
        [name],
        |row| row.get(0),
    )?)
}

pub fn load_import_profile(connection: &Connection, name: &str) -> Result<String, DbError> {
    connection
        .query_row(
            "SELECT config_toml FROM import_profiles WHERE name = ?1",
            [name],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| DbError::ProfileNotFound(name.to_owned()))
}

pub fn list_import_profiles(connection: &Connection) -> Result<Vec<SavedImportProfile>, DbError> {
    let mut statement = connection.prepare(
        "SELECT p.name, p.config_toml, a.external_key, a.display_name, a.currency
         FROM import_profiles p
         LEFT JOIN import_batches b ON b.id = (
            SELECT recent.id FROM import_batches recent
            WHERE recent.profile_id = p.id
            ORDER BY recent.imported_at DESC, recent.id DESC LIMIT 1
         )
         LEFT JOIN accounts a ON a.id = b.account_id
         ORDER BY p.updated_at DESC, p.name",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(SavedImportProfile {
            name: row.get(0)?,
            config_toml: row.get(1)?,
            account_key: row.get(2)?,
            account_name: row.get(3)?,
            currency: row.get(4)?,
        })
    })?;
    rows.collect::<Result<_, _>>().map_err(DbError::from)
}

pub fn upsert_account(
    connection: &Connection,
    external_key: &str,
    display_name: &str,
    currency: &CurrencyCode,
) -> Result<i64, DbError> {
    connection.execute(
        "INSERT INTO accounts(external_key, display_name, currency)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(external_key) DO UPDATE SET display_name = excluded.display_name",
        params![external_key, display_name, currency.as_str()],
    )?;
    let (id, stored_currency): (i64, String) = connection.query_row(
        "SELECT id, currency FROM accounts WHERE external_key = ?1",
        [external_key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if stored_currency != currency.as_str() {
        return Err(DbError::InvalidValue(format!(
            "account {external_key:?} already uses {stored_currency}, not {currency}"
        )));
    }
    Ok(id)
}

pub fn set_scenario(
    connection: &Connection,
    name: &str,
    currency: &CurrencyCode,
    reserve_minor: i64,
    explicit_assets: Option<(i64, NaiveDate)>,
) -> Result<Scenario, DbError> {
    let (assets_minor, assets_as_of) = explicit_assets
        .map(|(amount, date)| (Some(amount), Some(date.to_string())))
        .unwrap_or((None, None));
    connection.execute(
        "INSERT INTO scenarios(
            name, currency, reserve_minor, explicit_assets_minor, assets_as_of
         ) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name) DO UPDATE SET
            currency = excluded.currency,
            reserve_minor = excluded.reserve_minor,
            explicit_assets_minor = excluded.explicit_assets_minor,
            assets_as_of = excluded.assets_as_of,
            updated_at = CURRENT_TIMESTAMP",
        params![
            name,
            currency.as_str(),
            reserve_minor,
            assets_minor,
            assets_as_of
        ],
    )?;
    get_scenario(connection, name)
}

pub fn get_scenario(connection: &Connection, name: &str) -> Result<Scenario, DbError> {
    let raw = connection
        .query_row(
            "SELECT id, name, currency, reserve_minor, explicit_assets_minor,
                    assets_as_of, max_horizon_days
             FROM scenarios WHERE name = ?1",
            [name],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| DbError::ScenarioNotFound(name.to_owned()))?;
    Ok(Scenario {
        id: raw.0,
        name: raw.1,
        currency: CurrencyCode::new(raw.2)?,
        reserve_minor: raw.3,
        explicit_assets_minor: raw.4,
        assets_as_of: raw
            .5
            .map(|value| parse_stored_date("scenarios.assets_as_of", value))
            .transpose()?,
        max_horizon_days: u32::try_from(raw.6)
            .map_err(|_| DbError::InvalidValue("negative max_horizon_days".to_owned()))?,
    })
}

pub fn add_forecast_rule(
    connection: &Connection,
    scenario_name: &str,
    rule: &NewForecastRule,
) -> Result<i64, DbError> {
    let scenario = get_scenario(connection, scenario_name)?;
    if scenario.currency != rule.currency {
        return Err(DbError::InvalidValue(format!(
            "forecast rule currency {} does not match scenario currency {}",
            rule.currency, scenario.currency
        )));
    }
    let (cadence, day_of_month) = match rule.cadence {
        ForecastCadence::Once => ("once", None),
        ForecastCadence::Monthly { day_of_month } => ("monthly", Some(day_of_month)),
    };
    let direction = match rule.direction {
        CashFlowDirection::Income => "income",
        CashFlowDirection::Expense => "expense",
    };
    let evidence_json = serde_json::to_string(&rule.evidence)?;
    connection.execute(
        "INSERT INTO forecast_rules(
            scenario_id, label, direction, amount_minor, currency, cadence,
            day_of_month, starts_on, ends_on, source, confidence_ppm, evidence_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                   'user_confirmed', 1000000, ?10)",
        params![
            scenario.id,
            rule.label,
            direction,
            rule.amount_minor,
            rule.currency.as_str(),
            cadence,
            day_of_month,
            rule.starts_on.to_string(),
            rule.ends_on.map(|date| date.to_string()),
            evidence_json,
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

pub fn annotate_transaction(
    connection: &Connection,
    transaction_id: i64,
    class: &str,
    note: Option<&str>,
) -> Result<(), DbError> {
    connection.execute(
        "INSERT INTO transaction_interpretations(
            transaction_id, class, source, confidence_ppm, note
         ) VALUES (?1, ?2, 'user_confirmed', 1000000, ?3)
         ON CONFLICT(transaction_id) DO UPDATE SET
            class = excluded.class,
            source = 'user_confirmed',
            confidence_ppm = 1000000,
            note = excluded.note,
            updated_at = CURRENT_TIMESTAMP",
        params![transaction_id, class, note],
    )?;
    Ok(())
}

pub fn list_transactions(
    connection: &Connection,
    limit: u32,
) -> Result<Vec<TransactionListItem>, DbError> {
    let mut statement = connection.prepare(
        "SELECT t.id, t.booked_on, a.display_name, t.description_raw,
                t.amount_minor, t.currency, t.balance_after_minor, i.class
         FROM actual_transactions t
         JOIN accounts a ON a.id = t.account_id
         LEFT JOIN transaction_interpretations i ON i.transaction_id = t.id
         ORDER BY t.booked_on DESC, t.id DESC
         LIMIT ?1",
    )?;
    let rows = statement.query_map([limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;
    rows.map(|row| {
        let row = row?;
        Ok(TransactionListItem {
            id: row.0,
            booked_on: parse_stored_date("actual_transactions.booked_on", row.1)?,
            account: row.2,
            description: row.3,
            amount_minor: row.4,
            currency: row.5,
            balance_after_minor: row.6,
            interpretation: row.7,
        })
    })
    .collect()
}

pub fn list_unreviewed_outflows(
    connection: &Connection,
    limit: u32,
) -> Result<Vec<TransactionListItem>, DbError> {
    let mut statement = connection.prepare(
        "SELECT t.id, t.booked_on, a.display_name, t.description_raw,
                t.amount_minor, t.currency, t.balance_after_minor, NULL
         FROM actual_transactions t
         JOIN accounts a ON a.id = t.account_id
         LEFT JOIN transaction_interpretations i ON i.transaction_id = t.id
         WHERE t.amount_minor < 0 AND i.transaction_id IS NULL
         ORDER BY ABS(t.amount_minor) DESC, t.booked_on DESC, t.id DESC
         LIMIT ?1",
    )?;
    let rows = statement.query_map([limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<i64>>(6)?,
        ))
    })?;
    rows.map(|row| {
        let row = row?;
        Ok(TransactionListItem {
            id: row.0,
            booked_on: parse_stored_date("actual_transactions.booked_on", row.1)?,
            account: row.2,
            description: row.3,
            amount_minor: row.4,
            currency: row.5,
            balance_after_minor: row.6,
            interpretation: None,
        })
    })
    .collect()
}

pub fn app_summary(connection: &Connection) -> Result<AppSummary, DbError> {
    let transaction_count: u64 =
        connection.query_row("SELECT COUNT(*) FROM actual_transactions", [], |row| {
            row.get(0)
        })?;
    let last_actual_raw: Option<String> = connection.query_row(
        "SELECT MAX(booked_on) FROM actual_transactions",
        [],
        |row| row.get(0),
    )?;
    let last_actual_data = last_actual_raw
        .map(|value| parse_stored_date("actual_transactions.booked_on", value))
        .transpose()?;
    let unresolved_outflow_count: u64 = connection.query_row(
        "SELECT COUNT(*) FROM actual_transactions t
         LEFT JOIN transaction_interpretations i ON i.transaction_id = t.id
         WHERE t.amount_minor < 0 AND i.transaction_id IS NULL",
        [],
        |row| row.get(0),
    )?;

    let mut account_statement = connection.prepare(
        "SELECT a.external_key, a.display_name, a.currency, COUNT(t.id), MAX(t.booked_on)
         FROM accounts a
         LEFT JOIN actual_transactions t ON t.account_id = a.id
         GROUP BY a.id ORDER BY a.display_name",
    )?;
    let account_rows = account_statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, u64>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let accounts = account_rows
        .map(|row| {
            let row = row?;
            Ok(AccountSummary {
                external_key: row.0,
                display_name: row.1,
                currency: row.2,
                transaction_count: row.3,
                last_actual_data: row
                    .4
                    .map(|value| parse_stored_date("actual_transactions.booked_on", value))
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;

    let mut scenario_statement = connection.prepare("SELECT name FROM scenarios ORDER BY name")?;
    let scenarios = scenario_statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(AppSummary {
        transaction_count,
        last_actual_data,
        accounts,
        scenarios,
        unresolved_outflow_count,
    })
}

pub fn list_forecast_rules(
    connection: &Connection,
    scenario_name: &str,
) -> Result<Vec<ForecastRuleListItem>, DbError> {
    let scenario = get_scenario(connection, scenario_name)?;
    let mut statement = connection.prepare(
        "SELECT id, label, direction, amount_minor, currency, cadence,
                day_of_month, starts_on, ends_on, source, evidence_json
         FROM forecast_rules WHERE scenario_id = ?1 ORDER BY starts_on, id",
    )?;
    let rows = statement.query_map([scenario.id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<u32>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
        ))
    })?;
    rows.map(|row| {
        let row = row?;
        Ok(ForecastRuleListItem {
            id: row.0,
            label: row.1,
            direction: row.2,
            amount_minor: row.3,
            currency: row.4,
            cadence: row.5,
            day_of_month: row.6,
            starts_on: parse_stored_date("forecast_rules.starts_on", row.7)?,
            ends_on: row
                .8
                .map(|value| parse_stored_date("forecast_rules.ends_on", value))
                .transpose()?,
            source: row.9,
            evidence: serde_json::from_str(&row.10)?,
        })
    })
    .collect()
}

pub fn delete_forecast_rule(
    connection: &Connection,
    scenario_name: &str,
    rule_id: i64,
) -> Result<bool, DbError> {
    let scenario = get_scenario(connection, scenario_name)?;
    let deleted = connection.execute(
        "DELETE FROM forecast_rules WHERE id = ?1 AND scenario_id = ?2",
        params![rule_id, scenario.id],
    )?;
    Ok(deleted == 1)
}

pub fn build_runway_input(
    connection: &Connection,
    scenario_name: &str,
    as_of_override: Option<NaiveDate>,
) -> Result<(i64, RunwayInput), DbError> {
    let scenario = get_scenario(connection, scenario_name)?;
    let last_actual = last_actual_date(connection, scenario.currency.as_str())?;
    let as_of = as_of_override
        .or(scenario.assets_as_of)
        .unwrap_or(last_actual);
    if as_of_override.is_some() && as_of > last_actual && scenario.assets_as_of != Some(as_of) {
        return Err(DbError::StaleBalanceOverride {
            requested: as_of,
            last_actual,
        });
    }
    let liquid_assets_minor = match scenario.explicit_assets_minor {
        Some(amount) if as_of_override.is_none() || scenario.assets_as_of == Some(as_of) => amount,
        _ => current_balances(connection, scenario.currency.as_str(), as_of)?.ok_or_else(|| {
            DbError::NoAccountBalance {
                currency: scenario.currency.to_string(),
                as_of,
            }
        })?,
    };
    let observed_through = last_actual.min(as_of);
    let historical_burn =
        historical_burn(connection, scenario.currency.as_str(), observed_through)?;
    let forecast_rules = load_forecast_rules(connection, &scenario)?;

    Ok((
        scenario.id,
        RunwayInput {
            as_of,
            liquid_assets: Money::new(liquid_assets_minor, scenario.currency.clone()),
            reserve: Money::new(scenario.reserve_minor, scenario.currency),
            historical_burn,
            forecast_rules,
            max_horizon_days: Some(scenario.max_horizon_days),
        },
    ))
}

pub fn save_runway_snapshot(
    connection: &Connection,
    scenario_id: i64,
    result: &RunwayResult,
) -> Result<i64, DbError> {
    let explanation = serde_json::to_string(result)?;
    if let Some((id, previous)) = connection
        .query_row(
            "SELECT id, explanation_json FROM runway_snapshots
             WHERE scenario_id = ?1 ORDER BY id DESC LIMIT 1",
            [scenario_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        && previous == explanation
    {
        return Ok(id);
    }
    connection.execute(
        "INSERT INTO runway_snapshots(
            scenario_id, as_of, last_actual_data, currency, runway_days,
            zero_date, liquid_assets_minor, reserve_minor,
            projected_balance_minor, historical_expense_applied_minor,
            explanation_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            scenario_id,
            result.as_of.to_string(),
            result.last_actual_data.to_string(),
            result.currency.as_str(),
            result.runway_days,
            result.zero_date.map(|date| date.to_string()),
            result.liquid_assets_minor,
            result.reserve_minor,
            result.projected_balance_minor,
            result.historical_expense_applied_minor,
            explanation,
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

fn last_actual_date(connection: &Connection, currency: &str) -> Result<NaiveDate, DbError> {
    let value = connection
        .query_row(
            "SELECT MAX(booked_on) FROM actual_transactions WHERE currency = ?1",
            [currency],
            |row| row.get::<_, Option<String>>(0),
        )?
        .ok_or_else(|| DbError::NoActualTransactions(currency.to_owned()))?;
    parse_stored_date("actual_transactions.booked_on", value)
}

fn current_balances(
    connection: &Connection,
    currency: &str,
    as_of: NaiveDate,
) -> Result<Option<i64>, DbError> {
    connection
        .query_row(
            "SELECT SUM(latest.balance_after_minor)
             FROM accounts a
             JOIN actual_transactions latest ON latest.id = (
                SELECT t.id FROM actual_transactions t
                WHERE t.account_id = a.id
                  AND t.booked_on <= ?2
                  AND t.balance_after_minor IS NOT NULL
                ORDER BY t.booked_on DESC, t.id DESC LIMIT 1
             )
             WHERE a.currency = ?1",
            params![currency, as_of.to_string()],
            |row| row.get(0),
        )
        .map_err(DbError::from)
}

fn historical_burn(
    connection: &Connection,
    currency: &str,
    observed_through: NaiveDate,
) -> Result<HistoricalBurn, DbError> {
    let observed_from_raw: String = connection
        .query_row(
            "SELECT MIN(booked_on) FROM actual_transactions
         WHERE currency = ?1 AND booked_on <= ?2",
            params![currency, observed_through.to_string()],
            |row| row.get::<_, Option<String>>(0),
        )?
        .ok_or_else(|| DbError::NoActualTransactions(currency.to_owned()))?;
    let observed_from = parse_stored_date("actual_transactions.booked_on", observed_from_raw)?;
    let observed_days_i64 = observed_through
        .signed_duration_since(observed_from)
        .num_days()
        .checked_add(1)
        .ok_or_else(|| DbError::InvalidValue("observation window overflow".to_owned()))?;
    let observed_days = u32::try_from(observed_days_i64)
        .map_err(|_| DbError::InvalidValue("invalid observation window".to_owned()))?;

    let mut statement = connection.prepare(
        "SELECT t.amount_minor, i.class
         FROM actual_transactions t
         LEFT JOIN transaction_interpretations i ON i.transaction_id = t.id
         WHERE t.currency = ?1 AND t.booked_on BETWEEN ?2 AND ?3",
    )?;
    let rows = statement.query_map(
        params![
            currency,
            observed_from.to_string(),
            observed_through.to_string()
        ],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?)),
    )?;
    let mut expense_minor: i128 = 0;
    let mut included = 0_u64;
    let mut excluded = 0_u64;
    for row in rows {
        let (amount, class) = row?;
        let class = class.as_deref();
        let excluded_class = matches!(class, Some("transfer" | "exceptional" | "income"));
        if amount < 0 && !excluded_class && class != Some("refund") {
            expense_minor += -i128::from(amount);
            included += 1;
        } else if amount > 0 && class == Some("refund") {
            expense_minor -= i128::from(amount);
            included += 1;
        } else {
            excluded += 1;
        }
    }
    let expense_minor = i64::try_from(expense_minor.max(0))
        .map_err(|_| DbError::InvalidValue("historical expense overflow".to_owned()))?;
    Ok(HistoricalBurn {
        expense_minor,
        observed_days,
        observed_from,
        observed_through,
        included_transaction_count: included,
        excluded_transaction_count: excluded,
    })
}

fn load_forecast_rules(
    connection: &Connection,
    scenario: &Scenario,
) -> Result<Vec<ForecastRule>, DbError> {
    let mut statement = connection.prepare(
        "SELECT id, label, direction, amount_minor, currency, cadence,
                day_of_month, starts_on, ends_on, source, confidence_ppm,
                evidence_json
         FROM forecast_rules WHERE scenario_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map([scenario.id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<u32>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, u32>(10)?,
            row.get::<_, String>(11)?,
        ))
    })?;
    rows.map(|row| {
        let row = row?;
        let direction = match row.2.as_str() {
            "income" => CashFlowDirection::Income,
            "expense" => CashFlowDirection::Expense,
            other => {
                return Err(DbError::InvalidValue(format!(
                    "invalid direction {other:?}"
                )));
            }
        };
        let cadence = match (row.5.as_str(), row.6) {
            ("once", None) => ForecastCadence::Once,
            ("monthly", Some(day_of_month)) => ForecastCadence::Monthly { day_of_month },
            other => return Err(DbError::InvalidValue(format!("invalid cadence {other:?}"))),
        };
        let provenance = match row.9.as_str() {
            "user_confirmed" => Provenance::UserConfirmed,
            "deterministic_estimate" => Provenance::DeterministicEstimate,
            other => return Err(DbError::InvalidValue(format!("invalid source {other:?}"))),
        };
        Ok(ForecastRule {
            id: format!("forecast_rule:{}", row.0),
            label: row.1,
            direction,
            amount_minor: row.3,
            currency: CurrencyCode::new(row.4)?,
            cadence,
            starts_on: parse_stored_date("forecast_rules.starts_on", row.7)?,
            ends_on: row
                .8
                .map(|value| parse_stored_date("forecast_rules.ends_on", value))
                .transpose()?,
            provenance,
            confidence_ppm: row.10,
            evidence: serde_json::from_str(&row.11)?,
        })
    })
    .collect()
}

fn parse_stored_date(field: &'static str, value: String) -> Result<NaiveDate, DbError> {
    NaiveDate::parse_from_str(&value, "%Y-%m-%d")
        .map_err(|_| DbError::InvalidStoredDate { field, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn schema_keeps_actuals_forecasts_and_corrections_separate() {
        let db = open_in_memory().unwrap();
        let tables: Vec<String> = {
            let mut statement = db
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert!(tables.contains(&"actual_transactions".to_owned()));
        assert!(tables.contains(&"forecast_rules".to_owned()));
        assert!(tables.contains(&"transaction_interpretations".to_owned()));
        assert!(tables.contains(&"raw_import_rows".to_owned()));
    }

    #[test]
    fn scenario_round_trip_preserves_minor_units() {
        let db = open_in_memory().unwrap();
        let currency = CurrencyCode::new("try").unwrap();
        let scenario = set_scenario(
            &db,
            "no-work",
            &currency,
            2_000_000,
            Some((13_200_000, date("2026-08-20"))),
        )
        .unwrap();
        assert_eq!(scenario.currency.as_str(), "TRY");
        assert_eq!(scenario.explicit_assets_minor, Some(13_200_000));
    }

    #[test]
    fn v1_database_migrates_to_profile_aware_reimports() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE accounts(
                id INTEGER PRIMARY KEY, external_key TEXT, display_name TEXT, currency TEXT
             );
             CREATE TABLE import_profiles(
                id INTEGER PRIMARY KEY, name TEXT, config_toml TEXT
             );
             CREATE TABLE import_batches(
                id INTEGER PRIMARY KEY,
                account_id INTEGER NOT NULL REFERENCES accounts(id),
                profile_id INTEGER NOT NULL REFERENCES import_profiles(id),
                source_name TEXT NOT NULL,
                content_sha256 TEXT NOT NULL,
                imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                row_count INTEGER NOT NULL DEFAULT 0,
                inserted_count INTEGER NOT NULL DEFAULT 0,
                duplicate_count INTEGER NOT NULL DEFAULT 0,
                error_count INTEGER NOT NULL DEFAULT 0,
                UNIQUE(account_id, content_sha256)
             );
             CREATE TABLE actual_transactions(
                id INTEGER PRIMARY KEY,
                source_batch_id INTEGER NOT NULL REFERENCES import_batches(id)
             );
             INSERT INTO accounts VALUES(1, 'main', 'Main', 'TRY');
             INSERT INTO import_profiles VALUES(1, 'bank', 'name = \"bank\"');
             INSERT INTO import_batches(
                id, account_id, profile_id, source_name, content_sha256
             ) VALUES(1, 1, 1, 'old.csv', 'content');
             INSERT INTO actual_transactions VALUES(1, 1);
             PRAGMA user_version = 1;",
        )
        .unwrap();

        migrate(&db).unwrap();
        let version: i64 = db
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let profile_digest: String = db
            .query_row(
                "SELECT profile_sha256 FROM import_batches WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let profile_snapshot: String = db
            .query_row(
                "SELECT profile_config_toml FROM import_batches WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let foreign_key_errors: i64 = db
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 2);
        assert_eq!(profile_digest, "legacy-profile-1");
        assert_eq!(profile_snapshot, "name = \"bank\"");
        assert_eq!(foreign_key_errors, 0);
    }
}
