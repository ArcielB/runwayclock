use anyhow::{Context, Result, anyhow, bail};
use chrono::{NaiveDate, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use runway_core::{
    CashFlowDirection, CurrencyCode, ForecastCadence, RunwayInput, RunwayResult, calculate_runway,
};
use runway_db::NewForecastRule;
use runway_import::{ImportProfile, ValueFormats, import_csv, parse_amount_minor, parse_profile};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "runwayclock",
    version,
    about = "Local, explainable no-work runway calculator"
)]
struct Cli {
    /// SQLite database path. Defaults to the platform user-data directory.
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create or migrate the local database.
    Init,
    /// Import a bank CSV using a mapping profile.
    Import {
        #[arg(long)]
        csv: PathBuf,
        /// TOML profile file. It is saved into SQLite under its profile name.
        #[arg(long, conflicts_with = "profile_name")]
        profile: Option<PathBuf>,
        /// Reuse a profile previously saved during an import.
        #[arg(long, conflicts_with = "profile")]
        profile_name: Option<String>,
        /// Stable identifier for this owned account, e.g. checking-try.
        #[arg(long)]
        account: String,
        #[arg(long)]
        account_name: Option<String>,
        #[arg(long, default_value = "TRY")]
        currency: String,
    },
    /// Create or update a calculation scenario.
    ScenarioSet {
        #[arg(long, default_value = "no-work")]
        name: String,
        #[arg(long, default_value = "TRY")]
        currency: String,
        /// Amount the liquid balance must stay above.
        #[arg(long)]
        reserve: String,
        /// Optional explicit liquid assets; otherwise latest imported balances are summed.
        #[arg(long, requires = "assets_as_of")]
        assets: Option<String>,
        #[arg(long, requires = "assets")]
        assets_as_of: Option<NaiveDate>,
    },
    /// Add a user-confirmed future cash-flow fact.
    FlowAdd {
        #[arg(long, default_value = "no-work")]
        scenario: String,
        #[arg(long)]
        label: String,
        #[arg(long, value_enum)]
        direction: DirectionArg,
        #[arg(long)]
        amount: String,
        #[arg(long, default_value = "TRY")]
        currency: String,
        #[arg(long, value_enum)]
        cadence: CadenceArg,
        #[arg(long)]
        starts_on: NaiveDate,
        #[arg(long)]
        ends_on: Option<NaiveDate>,
        #[arg(long, required_if_eq("cadence", "monthly"))]
        day_of_month: Option<u32>,
        /// Evidence labels such as "transaction:192". May be repeated.
        #[arg(long = "evidence")]
        evidence: Vec<String>,
    },
    /// Permanently record a user's interpretation of an actual transaction.
    Annotate {
        #[arg(long)]
        transaction: i64,
        #[arg(long, value_enum)]
        class: InterpretationArg,
        #[arg(long)]
        note: Option<String>,
    },
    /// Show recently imported actual transactions and their stable database IDs.
    Transactions {
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long)]
        json: bool,
    },
    /// Calculate, explain, store, and publish the current runway snapshot.
    Calculate {
        #[arg(long, default_value = "no-work")]
        scenario: String,
        /// Override the balance date. Explicit assets are ignored if their date differs.
        #[arg(long)]
        as_of: Option<NaiveDate>,
        /// Sanitized JSON path consumed by the GNOME extension.
        #[arg(long)]
        snapshot: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DirectionArg {
    Income,
    Expense,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CadenceArg {
    Once,
    Monthly,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "snake_case")]
enum InterpretationArg {
    FixedRecurrent,
    VariableRecurrent,
    IrregularRecurrent,
    Exceptional,
    Transfer,
    Income,
    Refund,
    Unknown,
}

impl InterpretationArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::FixedRecurrent => "fixed_recurrent",
            Self::VariableRecurrent => "variable_recurrent",
            Self::IrregularRecurrent => "irregular_recurrent",
            Self::Exceptional => "exceptional",
            Self::Transfer => "transfer",
            Self::Income => "income",
            Self::Refund => "refund",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Serialize)]
struct WidgetSnapshot {
    schema_version: u32,
    scenario: String,
    calculated_at: String,
    as_of: NaiveDate,
    runway_days: Option<u32>,
    zero_date: Option<NaiveDate>,
    display_duration: Option<String>,
    last_actual_data: NaiveDate,
    change_30d: Option<i32>,
    confidence: &'static str,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = cli.db.unwrap_or_else(default_db_path);
    ensure_parent(&db_path)?;

    match cli.command {
        Command::Init => {
            runway_db::open(&db_path)?;
            println!("Initialized {}", db_path.display());
        }
        Command::Import {
            csv,
            profile,
            profile_name,
            account,
            account_name,
            currency,
        } => {
            let mut db = runway_db::open(&db_path)?;
            let profile = load_profile(&db, profile.as_deref(), profile_name.as_deref())?;
            let currency = CurrencyCode::new(currency)?;
            let report = import_csv(
                &mut db,
                &csv,
                &profile,
                &account,
                account_name.as_deref().unwrap_or(&account),
                &currency,
            )?;
            if report.exact_reimport {
                println!(
                    "Already imported: 0 new transactions ({} rows reconciled).",
                    report.duplicates
                );
            } else {
                println!(
                    "Imported {} new transactions; {} duplicates; {} row errors.",
                    report.inserted, report.duplicates, report.errors
                );
            }
            println!("Saved import profile: {}", report.profile_name);
        }
        Command::ScenarioSet {
            name,
            currency,
            reserve,
            assets,
            assets_as_of,
        } => {
            let db = runway_db::open(&db_path)?;
            let currency = CurrencyCode::new(currency)?;
            let reserve_minor = parse_user_amount(&reserve)?;
            if reserve_minor < 0 {
                bail!("reserve cannot be negative");
            }
            let explicit_assets = assets
                .map(|value| parse_user_amount(&value))
                .transpose()?
                .zip(assets_as_of);
            let scenario =
                runway_db::set_scenario(&db, &name, &currency, reserve_minor, explicit_assets)?;
            println!(
                "Saved scenario {}: reserve {}, currency {}.",
                scenario.name,
                format_money(scenario.reserve_minor, &scenario.currency),
                scenario.currency
            );
        }
        Command::FlowAdd {
            scenario,
            label,
            direction,
            amount,
            currency,
            cadence,
            starts_on,
            ends_on,
            day_of_month,
            evidence,
        } => {
            let db = runway_db::open(&db_path)?;
            let amount_minor = parse_user_amount(&amount)?;
            if amount_minor <= 0 {
                bail!("forecast amount must be positive; use --direction for its sign");
            }
            let cadence = match cadence {
                CadenceArg::Once => {
                    if day_of_month.is_some() {
                        bail!("--day-of-month only applies to monthly flows");
                    }
                    ForecastCadence::Once
                }
                CadenceArg::Monthly => ForecastCadence::Monthly {
                    day_of_month: day_of_month
                        .filter(|day| (1..=31).contains(day))
                        .ok_or_else(|| {
                            anyhow!("monthly flows require --day-of-month from 1 to 31")
                        })?,
                },
            };
            let rule = NewForecastRule {
                label: label.clone(),
                direction: match direction {
                    DirectionArg::Income => CashFlowDirection::Income,
                    DirectionArg::Expense => CashFlowDirection::Expense,
                },
                amount_minor,
                currency: CurrencyCode::new(currency)?,
                cadence,
                starts_on,
                ends_on,
                evidence,
            };
            let id = runway_db::add_forecast_rule(&db, &scenario, &rule)?;
            println!("Saved user-confirmed forecast rule {id}: {label}");
        }
        Command::Annotate {
            transaction,
            class,
            note,
        } => {
            let db = runway_db::open(&db_path)?;
            runway_db::annotate_transaction(&db, transaction, class.as_str(), note.as_deref())?;
            println!(
                "Transaction {transaction} is permanently marked {} (user-confirmed).",
                class.as_str()
            );
        }
        Command::Transactions { limit, json } => {
            let db = runway_db::open(&db_path)?;
            let transactions = runway_db::list_transactions(&db, limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&transactions)?);
            } else {
                for transaction in transactions {
                    let currency = CurrencyCode::new(&transaction.currency)?;
                    println!(
                        "{}  {}  {:>14}  {}  {}",
                        transaction.id,
                        transaction.booked_on,
                        format_money(transaction.amount_minor, &currency),
                        transaction
                            .interpretation
                            .as_deref()
                            .unwrap_or("unreviewed"),
                        transaction.description
                    );
                }
            }
        }
        Command::Calculate {
            scenario,
            as_of,
            snapshot,
            json,
        } => {
            let db = runway_db::open(&db_path)?;
            let (scenario_id, input) = runway_db::build_runway_input(&db, &scenario, as_of)?;
            let result = calculate_runway(&input)?;
            runway_db::save_runway_snapshot(&db, scenario_id, &result)?;
            let snapshot_path = snapshot.unwrap_or_else(default_snapshot_path);
            publish_widget_snapshot(&snapshot_path, &scenario, &input, &result)?;
            if json {
                let output = serde_json::json!({
                    "actual_and_assumptions": input,
                    "result": result,
                    "widget_snapshot": snapshot_path,
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                print_explanation(&scenario, &input, &result, &snapshot_path);
            }
        }
    }
    Ok(())
}

fn load_profile(
    db: &rusqlite::Connection,
    path: Option<&Path>,
    saved_name: Option<&str>,
) -> Result<ImportProfile> {
    match (path, saved_name) {
        (Some(path), None) => {
            let source = fs::read_to_string(path)
                .with_context(|| format!("failed to read import profile {}", path.display()))?;
            Ok(parse_profile(&source)?)
        }
        (None, Some(name)) => Ok(parse_profile(&runway_db::load_import_profile(db, name)?)?),
        (None, None) => bail!("provide either --profile FILE or --profile-name NAME"),
        (Some(_), Some(_)) => unreachable!("clap rejects conflicting profile arguments"),
    }
}

fn parse_user_amount(value: &str) -> Result<i64> {
    let compact: String = value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let last_comma = compact.rfind(',');
    let last_dot = compact.rfind('.');
    let (decimal_separator, thousands_separator) = match (last_comma, last_dot) {
        (Some(comma), Some(dot)) if comma > dot => (',', Some('.')),
        (Some(_), Some(_)) => ('.', Some(',')),
        (Some(_), None) => (',', None),
        (None, Some(dot)) => {
            let fraction_digits = compact[dot + 1..]
                .chars()
                .filter(|character| character.is_ascii_digit())
                .count();
            if fraction_digits <= 2 {
                ('.', None)
            } else {
                (',', Some('.'))
            }
        }
        (None, None) => ('.', None),
    };
    let formats = ValueFormats {
        date: Vec::new(),
        decimal_separator,
        thousands_separator,
        minor_unit_digits: 2,
    };
    parse_amount_minor(&compact, &formats).map_err(Into::into)
}

fn print_explanation(
    scenario: &str,
    input: &RunwayInput,
    result: &RunwayResult,
    snapshot_path: &Path,
) {
    println!("RUNWAY");
    println!(
        "{}",
        result
            .display_duration
            .as_deref()
            .unwrap_or("beyond calculation horizon")
    );
    if let Some(zero_date) = result.zero_date {
        println!("Reserve first reached: {zero_date}");
    }
    println!("Actual data through: {}", result.last_actual_data);
    println!();
    println!("Why ({scenario})");
    println!(
        "  Liquid assets: {}",
        format_money(
            input.liquid_assets.amount_minor,
            &input.liquid_assets.currency
        )
    );
    println!(
        "  Historical external spending: {} over {} days ({} included, {} excluded)",
        format_money(
            input.historical_burn.expense_minor,
            &input.liquid_assets.currency
        ),
        input.historical_burn.observed_days,
        input.historical_burn.included_transaction_count,
        input.historical_burn.excluded_transaction_count,
    );
    let monthly_estimate = i128::from(input.historical_burn.expense_minor) * 30
        / i128::from(input.historical_burn.observed_days);
    println!(
        "  Baseline spending estimate (30-day equivalent): {}",
        format_money(monthly_estimate as i64, &input.liquid_assets.currency)
    );
    for contribution in &result.rule_contributions {
        println!(
            "  {}: {} across {} occurrence(s)",
            contribution.label,
            format_money(contribution.amount_minor, &input.liquid_assets.currency),
            contribution.occurrence_count
        );
    }
    println!(
        "  Reserve: {}",
        format_money(input.reserve.amount_minor, &input.reserve.currency)
    );
    if input.forecast_rules.is_empty() {
        println!("  Allowed future income: none (salary is never inferred)");
    }
    for warning in &result.warnings {
        println!("  Warning: {warning}");
    }
    println!();
    println!("Widget snapshot: {}", snapshot_path.display());
}

fn publish_widget_snapshot(
    path: &Path,
    scenario: &str,
    input: &RunwayInput,
    result: &RunwayResult,
) -> Result<()> {
    ensure_parent(path)?;
    let confidence = match input.historical_burn.observed_days {
        0..=29 => "low",
        30..=179 => "medium",
        _ => "high",
    };
    let snapshot = WidgetSnapshot {
        schema_version: 1,
        scenario: scenario.to_owned(),
        calculated_at: Utc::now().to_rfc3339(),
        as_of: result.as_of,
        runway_days: result.runway_days,
        zero_date: result.zero_date,
        display_duration: result.display_duration.clone(),
        last_actual_data: result.last_actual_data,
        change_30d: None,
        confidence,
    };
    let temporary_path = path.with_extension("json.tmp");
    fs::write(&temporary_path, serde_json::to_vec_pretty(&snapshot)?)?;
    fs::rename(&temporary_path, path)?;
    Ok(())
}

fn format_money(amount_minor: i64, currency: &CurrencyCode) -> String {
    let sign = if amount_minor < 0 { "-" } else { "" };
    let magnitude = i128::from(amount_minor).abs();
    format!(
        "{sign}{}.{:02} {}",
        magnitude / 100,
        magnitude % 100,
        currency
    )
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

fn default_db_path() -> PathBuf {
    user_data_dir().join("runwayclock/runwayclock.db")
}

fn default_snapshot_path() -> PathBuf {
    user_data_dir().join("runwayclock/widget.json")
}

fn user_data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".runwayclock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_amount_accepts_turkish_and_english_notation() {
        assert_eq!(parse_user_amount("6.500,00").unwrap(), 650_000);
        assert_eq!(parse_user_amount("6500.00").unwrap(), 650_000);
        assert_eq!(parse_user_amount("132000").unwrap(), 13_200_000);
    }
}
