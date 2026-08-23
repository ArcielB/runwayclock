use chrono::{NaiveDate, Utc};
use runway_core::{
    CashFlowDirection, ForecastCadence, RunwayInput, RunwayResult, calculate_runway,
};
use runway_db::{AppSummary, ForecastRuleListItem, NewForecastRule, Scenario, TransactionListItem};
use runway_import::{
    ImportProfile, ImportReport, StatementPreview, import_csv, parse_flexible_amount_minor,
    parse_profile, preview_csv, profile_matches_headers,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{Manager, State};

#[derive(Debug, Clone)]
struct AppPaths {
    database: PathBuf,
    widget_snapshot: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewRequest {
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewResponse {
    source_name: String,
    headers: Vec<String>,
    sample_rows: Vec<Vec<String>>,
    profile: ImportProfile,
    matched_saved_profile: bool,
    account_key: String,
    account_name: String,
    currency: String,
    accounts: Vec<runway_db::AccountSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportStatementRequest {
    path: String,
    profile: ImportProfile,
    account_key: String,
    account_name: String,
    currency: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioRequest {
    name: String,
    currency: String,
    reserve: String,
    assets: Option<String>,
    assets_as_of: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowRequest {
    scenario: String,
    label: String,
    direction: String,
    amount: String,
    currency: String,
    cadence: String,
    starts_on: NaiveDate,
    ends_on: Option<NaiveDate>,
    day_of_month: Option<u32>,
    #[serde(default)]
    evidence: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnnotationRequest {
    transaction_id: i64,
    class: String,
    note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunwayView {
    input: RunwayInput,
    result: RunwayResult,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewCandidate {
    transaction: TransactionListItem,
    estimated_effect_days: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardResponse {
    summary: AppSummary,
    scenario: Option<Scenario>,
    forecast_rules: Vec<ForecastRuleListItem>,
    runway: Option<RunwayView>,
    runway_error: Option<String>,
    review_candidates: Vec<ReviewCandidate>,
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

#[tauri::command]
fn get_dashboard(paths: State<'_, AppPaths>) -> Result<DashboardResponse, String> {
    let db = runway_db::open(&paths.database).map_err(display_error)?;
    let summary = runway_db::app_summary(&db).map_err(display_error)?;
    let scenario_name = if summary.scenarios.iter().any(|name| name == "no-work") {
        Some("no-work".to_owned())
    } else {
        summary.scenarios.first().cloned()
    };
    let scenario = scenario_name
        .as_deref()
        .map(|name| runway_db::get_scenario(&db, name))
        .transpose()
        .map_err(display_error)?;
    let forecast_rules = scenario_name
        .as_deref()
        .map(|name| runway_db::list_forecast_rules(&db, name))
        .transpose()
        .map_err(display_error)?
        .unwrap_or_default();

    let mut runway = None;
    let mut runway_error = None;
    if let Some(name) = scenario_name.as_deref() {
        match runway_db::build_runway_input(&db, name, None) {
            Ok((scenario_id, input)) => match calculate_runway(&input) {
                Ok(result) => {
                    if let Err(error) = runway_db::save_runway_snapshot(&db, scenario_id, &result) {
                        runway_error = Some(error.to_string());
                    } else if let Err(error) =
                        publish_widget_snapshot(&paths.widget_snapshot, name, &input, &result)
                    {
                        runway_error = Some(error);
                    }
                    runway = Some(RunwayView { input, result });
                }
                Err(error) => runway_error = Some(error.to_string()),
            },
            Err(error) => runway_error = Some(error.to_string()),
        }
    }

    let review_candidates = runway_db::list_unreviewed_outflows(&db, 8)
        .map_err(display_error)?
        .into_iter()
        .map(|transaction| {
            let estimated_effect_days = runway.as_ref().and_then(|view| {
                let burn = &view.input.historical_burn;
                (burn.expense_minor > 0).then(|| {
                    let days = i128::from(transaction.amount_minor.unsigned_abs())
                        * i128::from(burn.observed_days)
                        / i128::from(burn.expense_minor);
                    u32::try_from(days.max(1)).unwrap_or(u32::MAX)
                })
            });
            ReviewCandidate {
                transaction,
                estimated_effect_days,
            }
        })
        .collect();

    Ok(DashboardResponse {
        summary,
        scenario,
        forecast_rules,
        runway,
        runway_error,
        review_candidates,
    })
}

#[tauri::command]
fn preview_statement(
    request: PreviewRequest,
    paths: State<'_, AppPaths>,
) -> Result<PreviewResponse, String> {
    let detected = preview_csv(&request.path).map_err(display_error)?;
    let db = runway_db::open(&paths.database).map_err(display_error)?;
    let saved_profiles = runway_db::list_import_profiles(&db).map_err(display_error)?;
    let accounts = runway_db::app_summary(&db).map_err(display_error)?.accounts;
    let matched = saved_profiles.into_iter().find_map(|saved| {
        let profile = parse_profile(&saved.config_toml).ok()?;
        profile_matches_headers(&profile, &detected.headers).then_some((profile, saved))
    });
    let StatementPreview {
        source_name,
        headers,
        sample_rows,
        suggested_profile,
    } = detected;
    let fallback_key = slugify(&suggested_profile.name);
    let (profile, matched_saved_profile, account_key, account_name, currency) = match matched {
        Some((mut profile, saved)) => {
            // Encoding, delimiter, and preamble can change between exports even
            // when the logical statement shape remains the same.
            profile.encoding = suggested_profile.encoding;
            profile.delimiter = suggested_profile.delimiter;
            profile.skip_rows = suggested_profile.skip_rows;
            (
                profile,
                true,
                saved.account_key.unwrap_or_else(|| fallback_key.clone()),
                saved
                    .account_name
                    .unwrap_or_else(|| "Main account".to_owned()),
                saved.currency.unwrap_or_else(|| "TRY".to_owned()),
            )
        }
        None => (
            suggested_profile,
            false,
            fallback_key,
            "Main account".to_owned(),
            "TRY".to_owned(),
        ),
    };
    Ok(PreviewResponse {
        source_name,
        headers,
        sample_rows,
        profile,
        matched_saved_profile,
        account_key,
        account_name,
        currency,
        accounts,
    })
}

#[tauri::command]
fn import_statement(
    request: ImportStatementRequest,
    paths: State<'_, AppPaths>,
) -> Result<ImportReport, String> {
    if request.account_key.trim().is_empty() || request.account_name.trim().is_empty() {
        return Err("Account name and identifier are required.".to_owned());
    }
    let currency = runway_core::CurrencyCode::new(&request.currency).map_err(display_error)?;
    let mut db = runway_db::open(&paths.database).map_err(display_error)?;
    let report = import_csv(
        &mut db,
        &request.path,
        &request.profile,
        request.account_key.trim(),
        request.account_name.trim(),
        &currency,
    )
    .map_err(display_error)?;
    if matches!(
        runway_db::get_scenario(&db, "no-work"),
        Err(runway_db::DbError::ScenarioNotFound(_))
    ) {
        runway_db::set_scenario(&db, "no-work", &currency, 0, None).map_err(display_error)?;
    }
    Ok(report)
}

#[tauri::command]
fn save_scenario(request: ScenarioRequest, paths: State<'_, AppPaths>) -> Result<Scenario, String> {
    let db = runway_db::open(&paths.database).map_err(display_error)?;
    let currency = runway_core::CurrencyCode::new(&request.currency).map_err(display_error)?;
    let reserve_minor = parse_flexible_amount_minor(&request.reserve).map_err(display_error)?;
    if reserve_minor < 0 {
        return Err("Reserve cannot be negative.".to_owned());
    }
    let explicit_assets = match (request.assets, request.assets_as_of) {
        (Some(assets), Some(as_of)) if !assets.trim().is_empty() => Some((
            parse_flexible_amount_minor(&assets).map_err(display_error)?,
            as_of,
        )),
        (None, None) => None,
        (Some(assets), None) if assets.trim().is_empty() => None,
        (Some(_), None) => return Err("Asset amount requires an as-of date.".to_owned()),
        (None, Some(_)) => return Err("Asset date requires an asset amount.".to_owned()),
        (Some(_), Some(_)) => None,
    };
    runway_db::set_scenario(
        &db,
        request.name.trim(),
        &currency,
        reserve_minor,
        explicit_assets,
    )
    .map_err(display_error)
}

#[tauri::command]
fn add_flow(request: FlowRequest, paths: State<'_, AppPaths>) -> Result<i64, String> {
    let db = runway_db::open(&paths.database).map_err(display_error)?;
    let direction = match request.direction.as_str() {
        "income" => CashFlowDirection::Income,
        "expense" => CashFlowDirection::Expense,
        _ => return Err("Direction must be income or expense.".to_owned()),
    };
    let cadence = match request.cadence.as_str() {
        "once" => ForecastCadence::Once,
        "monthly" => ForecastCadence::Monthly {
            day_of_month: request
                .day_of_month
                .filter(|day| (1..=31).contains(day))
                .ok_or_else(|| "Monthly facts need a day from 1 to 31.".to_owned())?,
        },
        _ => return Err("Cadence must be once or monthly.".to_owned()),
    };
    let amount_minor = parse_flexible_amount_minor(&request.amount).map_err(display_error)?;
    if amount_minor <= 0 {
        return Err("Amount must be positive; choose income or expense separately.".to_owned());
    }
    let rule = NewForecastRule {
        label: request.label,
        direction,
        amount_minor,
        currency: runway_core::CurrencyCode::new(request.currency).map_err(display_error)?,
        cadence,
        starts_on: request.starts_on,
        ends_on: request.ends_on,
        evidence: request.evidence,
    };
    runway_db::add_forecast_rule(&db, &request.scenario, &rule).map_err(display_error)
}

#[tauri::command]
fn remove_flow(scenario: String, rule_id: i64, paths: State<'_, AppPaths>) -> Result<bool, String> {
    let db = runway_db::open(&paths.database).map_err(display_error)?;
    runway_db::delete_forecast_rule(&db, &scenario, rule_id).map_err(display_error)
}

#[tauri::command]
fn annotate_transaction(
    request: AnnotationRequest,
    paths: State<'_, AppPaths>,
) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "fixed_recurrent",
        "variable_recurrent",
        "irregular_recurrent",
        "exceptional",
        "transfer",
        "income",
        "refund",
        "unknown",
    ];
    if !ALLOWED.contains(&request.class.as_str()) {
        return Err("Unknown transaction interpretation.".to_owned());
    }
    let db = runway_db::open(&paths.database).map_err(display_error)?;
    runway_db::annotate_transaction(
        &db,
        request.transaction_id,
        &request.class,
        request.note.as_deref(),
    )
    .map_err(display_error)
}

fn publish_widget_snapshot(
    path: &Path,
    scenario: &str,
    input: &RunwayInput,
    result: &RunwayResult,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(display_error)?;
    }
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
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&snapshot).map_err(display_error)?,
    )
    .map_err(display_error)?;
    fs::rename(temporary, path).map_err(display_error)
}

fn slugify(value: &str) -> String {
    let slug = value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "main-account".to_owned()
    } else {
        slug
    }
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));
    builder
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app.path().data_dir()?.join("runwayclock");
            fs::create_dir_all(&app_data)?;
            let widget_snapshot = app_data.join("widget.json");
            let database = app_data.join("runwayclock.db");
            runway_db::open(&database)?;
            app.manage(AppPaths {
                database,
                widget_snapshot,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            preview_statement,
            import_statement,
            save_scenario,
            add_flow,
            remove_flow,
            annotate_transaction,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run RunwayClock");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_name_becomes_stable_account_key() {
        assert_eq!(slugify("My Bank – TRY"), "my-bank-try");
        assert_eq!(slugify("---"), "main-account");
    }
}
