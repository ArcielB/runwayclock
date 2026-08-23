//! Bank-agnostic CSV ingestion with saved profiles, raw-row preservation, and
//! idempotent reconciliation.

use chrono::NaiveDate;
use encoding_rs::{Encoding, UTF_8};
use runway_core::CurrencyCode;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Csv(#[from] csv::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Database(#[from] runway_db::DbError),
    #[error(transparent)]
    TomlDecode(#[from] toml::de::Error),
    #[error(transparent)]
    TomlEncode(#[from] toml::ser::Error),
    #[error("unknown encoding {0:?}")]
    UnknownEncoding(String),
    #[error("input contains bytes that are invalid in {0}")]
    InvalidEncoding(String),
    #[error("delimiter must be one ASCII character or 'auto'")]
    InvalidDelimiter,
    #[error("profile maps neither an amount column nor debit/credit columns")]
    MissingAmountMapping,
    #[error("profile maps both a signed amount and debit/credit columns")]
    AmbiguousAmountMapping,
    #[error("mapped column {0:?} was not found in the CSV header")]
    MissingColumn(String),
    #[error("invalid date {value:?}; expected one of {formats:?}")]
    InvalidDate { value: String, formats: Vec<String> },
    #[error("invalid money value {value:?}: {reason}")]
    InvalidMoney { value: String, reason: String },
    #[error("amount and balance formats must use different decimal and thousands separators")]
    ConflictingSeparators,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportProfile {
    pub name: String,
    #[serde(default = "default_delimiter")]
    pub delimiter: String,
    #[serde(default = "default_encoding")]
    pub encoding: String,
    #[serde(default)]
    pub skip_rows: usize,
    pub columns: ColumnMapping,
    #[serde(default)]
    pub formats: ValueFormats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnMapping {
    pub date: String,
    pub description: String,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub debit: Option<String>,
    #[serde(default)]
    pub credit: Option<String>,
    #[serde(default)]
    pub balance: Option<String>,
    #[serde(default)]
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueFormats {
    #[serde(default = "default_date_formats")]
    pub date: Vec<String>,
    #[serde(default = "default_decimal_separator")]
    pub decimal_separator: char,
    #[serde(default = "default_thousands_separator")]
    pub thousands_separator: Option<char>,
    #[serde(default = "default_minor_unit_digits")]
    pub minor_unit_digits: u32,
}

impl Default for ValueFormats {
    fn default() -> Self {
        Self {
            date: default_date_formats(),
            decimal_separator: default_decimal_separator(),
            thousands_separator: default_thousands_separator(),
            minor_unit_digits: default_minor_unit_digits(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub batch_id: i64,
    pub profile_name: String,
    pub account_key: String,
    pub rows: u64,
    pub inserted: u64,
    pub duplicates: u64,
    pub errors: u64,
    pub row_errors: Vec<ImportRowError>,
    pub exact_reimport: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportRowError {
    pub row_number: u64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatementPreview {
    pub source_name: String,
    pub headers: Vec<String>,
    pub sample_rows: Vec<Vec<String>>,
    pub suggested_profile: ImportProfile,
}

#[derive(Debug)]
struct ParsedRow {
    booked_on: NaiveDate,
    description_raw: String,
    description_normalized: String,
    amount_minor: i64,
    balance_after_minor: Option<i64>,
    external_id: Option<String>,
}

pub fn parse_profile(source: &str) -> Result<ImportProfile, ImportError> {
    let profile: ImportProfile = toml::from_str(source)?;
    validate_profile(&profile)?;
    Ok(profile)
}

/// Inspect a statement without writing any financial data. The preview detects
/// common preamble rows, encoding, delimiter, Turkish/English column names,
/// date formats, and decimal conventions. Every suggestion remains editable.
pub fn preview_csv(csv_path: impl AsRef<Path>) -> Result<StatementPreview, ImportError> {
    let csv_path = csv_path.as_ref();
    let bytes = fs::read(csv_path)?;
    let (encoding, decoded) = decode_for_preview(&bytes)?;
    let (skip_rows, delimiter) = detect_header_row(&decoded)?;
    let tabular_source = skip_leading_lines(&decoded, skip_rows);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(tabular_source.as_bytes());
    let headers_record = reader.headers()?.clone();
    let headers: Vec<String> = headers_record
        .iter()
        .map(|header| header.trim().trim_start_matches('\u{feff}').to_owned())
        .collect();
    let sample_rows: Vec<Vec<String>> = reader
        .records()
        .take(8)
        .map(|record| record.map(|record| record.iter().map(ToOwned::to_owned).collect()))
        .collect::<Result<_, _>>()?;
    let columns = suggest_columns(&headers);
    let formats = suggest_formats(&headers, &sample_rows, &columns);
    let source_name = csv_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("statement.csv")
        .to_owned();
    let profile_name = csv_path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("bank-statement")
        .to_owned();
    Ok(StatementPreview {
        source_name,
        headers,
        sample_rows,
        suggested_profile: ImportProfile {
            name: profile_name,
            delimiter: char::from(delimiter).to_string(),
            encoding,
            skip_rows,
            columns,
            formats,
        },
    })
}

pub fn profile_matches_headers(profile: &ImportProfile, headers: &[String]) -> bool {
    let contains = |name: &str| headers.iter().any(|header| header.trim() == name.trim());
    contains(&profile.columns.date)
        && contains(&profile.columns.description)
        && profile.columns.amount.as_deref().is_none_or(&contains)
        && profile.columns.debit.as_deref().is_none_or(&contains)
        && profile.columns.credit.as_deref().is_none_or(&contains)
        && profile.columns.balance.as_deref().is_none_or(&contains)
        && profile
            .columns
            .transaction_id
            .as_deref()
            .is_none_or(&contains)
}

pub fn import_csv(
    connection: &mut Connection,
    csv_path: impl AsRef<Path>,
    profile: &ImportProfile,
    account_key: &str,
    account_name: &str,
    currency: &CurrencyCode,
) -> Result<ImportReport, ImportError> {
    validate_profile(profile)?;
    let csv_path = csv_path.as_ref();
    let bytes = fs::read(csv_path)?;
    let content_sha256 = sha256_hex(&bytes);
    let source_name = csv_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("statement.csv");
    let canonical_profile = toml::to_string_pretty(profile)?;
    let profile_sha256 = sha256_hex(canonical_profile.as_bytes());
    let profile_id = runway_db::save_import_profile(connection, &profile.name, &canonical_profile)?;
    let account_id = runway_db::upsert_account(connection, account_key, account_name, currency)?;

    if let Some(report) = existing_batch(
        connection,
        account_id,
        &content_sha256,
        &profile_sha256,
        &profile.name,
        account_key,
    )? {
        return Ok(report);
    }

    let decoded = decode(&bytes, &profile.encoding)?;
    let tabular_source = skip_leading_lines(&decoded, profile.skip_rows);
    let delimiter = resolve_delimiter(&profile.delimiter, tabular_source)?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(tabular_source.as_bytes());
    let headers = reader.headers()?.clone();
    let indexes = ResolvedColumns::new(&headers, &profile.columns)?;

    let transaction = connection.transaction()?;
    transaction.execute(
        "INSERT INTO import_batches(
            account_id, profile_id, source_name, content_sha256, profile_sha256,
            profile_config_toml
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            account_id,
            profile_id,
            source_name,
            content_sha256,
            profile_sha256,
            canonical_profile
        ],
    )?;
    let batch_id = transaction.last_insert_rowid();
    let mut report = ImportReport {
        batch_id,
        profile_name: profile.name.clone(),
        account_key: account_key.to_owned(),
        rows: 0,
        inserted: 0,
        duplicates: 0,
        errors: 0,
        row_errors: Vec::new(),
        exact_reimport: false,
    };

    for (index, record) in reader.records().enumerate() {
        let row_number = index + 2 + profile.skip_rows;
        report.rows += 1;
        match record {
            Ok(record) => {
                let raw_json = raw_row_json(&headers, &record)?;
                match parse_row(&record, &indexes, profile) {
                    Ok(parsed) => {
                        let dedup_key = dedup_key(account_key, &parsed);
                        let inserted = transaction.execute(
                            "INSERT INTO actual_transactions(
                                account_id, booked_on, description_raw,
                                description_normalized, amount_minor, currency,
                                balance_after_minor, external_id, dedup_key,
                                source_batch_id
                             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                             ON CONFLICT(account_id, dedup_key) DO NOTHING",
                            params![
                                account_id,
                                parsed.booked_on.to_string(),
                                parsed.description_raw,
                                parsed.description_normalized,
                                parsed.amount_minor,
                                currency.as_str(),
                                parsed.balance_after_minor,
                                parsed.external_id,
                                dedup_key,
                                batch_id,
                            ],
                        )?;
                        let transaction_id = if inserted == 1 {
                            report.inserted += 1;
                            transaction.last_insert_rowid()
                        } else {
                            report.duplicates += 1;
                            transaction.query_row(
                                "SELECT id FROM actual_transactions
                                 WHERE account_id = ?1 AND dedup_key = ?2",
                                params![account_id, dedup_key],
                                |row| row.get(0),
                            )?
                        };
                        transaction.execute(
                            "INSERT INTO raw_import_rows(
                                batch_id, row_number, raw_json, transaction_id
                             ) VALUES (?1, ?2, ?3, ?4)",
                            params![batch_id, row_number as i64, raw_json, transaction_id],
                        )?;
                    }
                    Err(error) => {
                        report.errors += 1;
                        report.row_errors.push(ImportRowError {
                            row_number: row_number as u64,
                            message: error.to_string(),
                        });
                        transaction.execute(
                            "INSERT INTO raw_import_rows(
                                batch_id, row_number, raw_json, parse_error
                             ) VALUES (?1, ?2, ?3, ?4)",
                            params![batch_id, row_number as i64, raw_json, error.to_string()],
                        )?;
                    }
                }
            }
            Err(error) => {
                report.errors += 1;
                report.row_errors.push(ImportRowError {
                    row_number: row_number as u64,
                    message: error.to_string(),
                });
                let placeholder = serde_json::json!({ "csv_error": error.to_string() });
                transaction.execute(
                    "INSERT INTO raw_import_rows(
                        batch_id, row_number, raw_json, parse_error
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        batch_id,
                        row_number as i64,
                        placeholder.to_string(),
                        error.to_string()
                    ],
                )?;
            }
        }
    }
    transaction.execute(
        "UPDATE import_batches SET
            row_count = ?2, inserted_count = ?3, duplicate_count = ?4,
            error_count = ?5
         WHERE id = ?1",
        params![
            batch_id,
            report.rows,
            report.inserted,
            report.duplicates,
            report.errors
        ],
    )?;
    transaction.commit()?;
    Ok(report)
}

pub fn parse_amount_minor(value: &str, formats: &ValueFormats) -> Result<i64, ImportError> {
    if formats.thousands_separator == Some(formats.decimal_separator) {
        return Err(ImportError::ConflictingSeparators);
    }
    let original = value.trim();
    if original.is_empty() {
        return Err(invalid_money(value, "empty value"));
    }
    let negative_parentheses = original.starts_with('(') && original.ends_with(')');
    let mut filtered: String = original
        .chars()
        .filter(|character| {
            character.is_ascii_digit()
                || *character == formats.decimal_separator
                || formats.thousands_separator == Some(*character)
                || matches!(character, '+' | '-')
        })
        .collect();
    let trailing_negative = filtered.ends_with('-');
    if trailing_negative {
        filtered.pop();
        filtered.insert(0, '-');
    }
    if let Some(separator) = formats.thousands_separator {
        filtered = filtered.replace(separator, "");
    }
    let negative = negative_parentheses || filtered.starts_with('-');
    let unsigned = filtered.trim_start_matches(['+', '-']);
    if unsigned.is_empty() || !unsigned.chars().any(|character| character.is_ascii_digit()) {
        return Err(invalid_money(value, "no digits"));
    }
    if unsigned.matches(formats.decimal_separator).count() > 1 {
        return Err(invalid_money(value, "multiple decimal separators"));
    }
    let mut pieces = unsigned.split(formats.decimal_separator);
    let whole = pieces.next().unwrap_or("0");
    let fraction = pieces.next().unwrap_or("");
    if pieces.next().is_some()
        || !whole.chars().all(|c| c.is_ascii_digit())
        || !fraction.chars().all(|c| c.is_ascii_digit())
    {
        return Err(invalid_money(value, "unexpected characters"));
    }
    if fraction.len() > formats.minor_unit_digits as usize {
        return Err(invalid_money(
            value,
            "more fractional digits than the profile permits",
        ));
    }
    let scale = 10_i128
        .checked_pow(formats.minor_unit_digits)
        .ok_or_else(|| invalid_money(value, "minor-unit scale overflow"))?;
    let whole_value = if whole.is_empty() {
        0
    } else {
        whole
            .parse::<i128>()
            .map_err(|_| invalid_money(value, "whole-number overflow"))?
    };
    let mut fraction_owned = fraction.to_owned();
    while fraction_owned.len() < formats.minor_unit_digits as usize {
        fraction_owned.push('0');
    }
    let fraction_value = if fraction_owned.is_empty() {
        0
    } else {
        fraction_owned
            .parse::<i128>()
            .map_err(|_| invalid_money(value, "fraction overflow"))?
    };
    let magnitude = whole_value
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fraction_value))
        .ok_or_else(|| invalid_money(value, "amount overflow"))?;
    let signed = if negative { -magnitude } else { magnitude };
    i64::try_from(signed).map_err(|_| invalid_money(value, "amount is outside the supported range"))
}

/// Parse a user-entered two-decimal amount in either `6.500,00` or `6500.00`
/// notation. Statement imports remain profile-driven; this is only for explicit
/// facts entered by a person.
pub fn parse_flexible_amount_minor(value: &str) -> Result<i64, ImportError> {
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
    parse_amount_minor(
        &compact,
        &ValueFormats {
            date: Vec::new(),
            decimal_separator,
            thousands_separator,
            minor_unit_digits: 2,
        },
    )
}

fn validate_profile(profile: &ImportProfile) -> Result<(), ImportError> {
    let uses_signed = profile.columns.amount.is_some();
    let uses_split = profile.columns.debit.is_some() || profile.columns.credit.is_some();
    match (uses_signed, uses_split) {
        (false, false) => return Err(ImportError::MissingAmountMapping),
        (true, true) => return Err(ImportError::AmbiguousAmountMapping),
        _ => {}
    }
    if profile.formats.thousands_separator == Some(profile.formats.decimal_separator) {
        return Err(ImportError::ConflictingSeparators);
    }
    if profile.delimiter != "auto"
        && (profile.delimiter.len() != 1 || !profile.delimiter.is_ascii())
    {
        return Err(ImportError::InvalidDelimiter);
    }
    Ok(())
}

fn suggest_columns(headers: &[String]) -> ColumnMapping {
    let date = find_header(
        headers,
        &[
            "tarih",
            "islem tarihi",
            "date",
            "transaction date",
            "booking date",
        ],
    )
    .unwrap_or_default();
    let description = find_header(
        headers,
        &[
            "aciklama",
            "islem aciklamasi",
            "description",
            "details",
            "merchant",
            "memo",
        ],
    )
    .unwrap_or_default();
    let amount = find_header(
        headers,
        &["islem tutari", "tutar", "amount", "transaction amount"],
    );
    let (debit, credit) = if amount.is_none() {
        (
            find_header(headers, &["borc", "debit", "withdrawal", "cikan"]),
            find_header(headers, &["alacak", "credit", "deposit", "giren"]),
        )
    } else {
        (None, None)
    };
    ColumnMapping {
        date,
        description,
        amount,
        debit,
        credit,
        balance: find_header(headers, &["bakiye", "balance", "running balance"]),
        transaction_id: find_header(
            headers,
            &[
                "islem no",
                "islem id",
                "referans no",
                "transaction id",
                "reference no",
                "reference",
            ],
        ),
    }
}

fn suggest_formats(
    headers: &[String],
    rows: &[Vec<String>],
    columns: &ColumnMapping,
) -> ValueFormats {
    let date_index = headers.iter().position(|header| header == &columns.date);
    let date_values: Vec<&str> = date_index
        .into_iter()
        .flat_map(|index| {
            rows.iter()
                .filter_map(move |row| row.get(index).map(String::as_str))
        })
        .filter(|value| !value.trim().is_empty())
        .collect();
    let date_candidates = ["%d.%m.%Y", "%d/%m/%Y", "%Y-%m-%d", "%m/%d/%Y"];
    let mut date: Vec<String> = date_candidates
        .iter()
        .filter(|format| {
            !date_values.is_empty()
                && date_values
                    .iter()
                    .all(|value| NaiveDate::parse_from_str(value.trim(), format).is_ok())
        })
        .map(|format| (*format).to_owned())
        .collect();
    if date.is_empty() {
        date = default_date_formats();
    }

    let amount_column = columns
        .amount
        .as_ref()
        .or(columns.debit.as_ref())
        .or(columns.credit.as_ref());
    let amount_index =
        amount_column.and_then(|name| headers.iter().position(|header| header == name));
    let values: Vec<&str> = amount_index
        .into_iter()
        .flat_map(|index| {
            rows.iter()
                .filter_map(move |row| row.get(index).map(String::as_str))
        })
        .filter(|value| !value.trim().is_empty())
        .collect();
    let mut comma_decimal = 0_u32;
    let mut dot_decimal = 0_u32;
    for value in values {
        match (value.rfind(','), value.rfind('.')) {
            (Some(comma), Some(dot)) if comma > dot => comma_decimal += 2,
            (Some(_), Some(_)) => dot_decimal += 2,
            (Some(comma), None) if trailing_digit_count(value, comma) <= 2 => comma_decimal += 1,
            (None, Some(dot)) if trailing_digit_count(value, dot) <= 2 => dot_decimal += 1,
            _ => {}
        }
    }
    let looks_turkish = headers.iter().any(|header| {
        let key = normalized_header(header);
        ["tarih", "aciklama", "tutar", "bakiye"]
            .iter()
            .any(|word| key.contains(word))
    });
    let decimal_separator =
        if comma_decimal > dot_decimal || (comma_decimal == dot_decimal && looks_turkish) {
            ','
        } else {
            '.'
        };
    ValueFormats {
        date,
        decimal_separator,
        thousands_separator: Some(if decimal_separator == ',' { '.' } else { ',' }),
        minor_unit_digits: 2,
    }
}

fn find_header(headers: &[String], aliases: &[&str]) -> Option<String> {
    headers
        .iter()
        .filter_map(|header| {
            let normalized = normalized_header(header);
            let score = aliases
                .iter()
                .map(|alias| {
                    if normalized == *alias {
                        100
                    } else if normalized.contains(alias) || alias.contains(&normalized) {
                        60
                    } else {
                        0
                    }
                })
                .max()
                .unwrap_or(0);
            (score > 0).then_some((score, header.clone()))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, header)| header)
}

fn normalized_header(value: &str) -> String {
    value
        .replace(['İ', 'I', 'ı'], "i")
        .replace(['Ş', 'ş'], "s")
        .replace(['Ğ', 'ğ'], "g")
        .replace(['Ü', 'ü'], "u")
        .replace(['Ö', 'ö'], "o")
        .replace(['Ç', 'ç'], "c")
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|piece| !piece.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn trailing_digit_count(value: &str, separator_index: usize) -> usize {
    value[separator_index + 1..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .count()
}

fn parse_row(
    record: &csv::StringRecord,
    columns: &ResolvedColumns,
    profile: &ImportProfile,
) -> Result<ParsedRow, ImportError> {
    let date_value = record.get(columns.date).unwrap_or_default().trim();
    let booked_on = profile
        .formats
        .date
        .iter()
        .find_map(|format| NaiveDate::parse_from_str(date_value, format).ok())
        .ok_or_else(|| ImportError::InvalidDate {
            value: date_value.to_owned(),
            formats: profile.formats.date.clone(),
        })?;
    let description_raw = record
        .get(columns.description)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let amount_minor = if let Some(index) = columns.amount {
        parse_amount_minor(record.get(index).unwrap_or_default(), &profile.formats)?
    } else {
        let debit = columns
            .debit
            .and_then(|index| nonempty(record.get(index)))
            .map(|value| parse_amount_minor(value, &profile.formats))
            .transpose()?
            .map(checked_magnitude)
            .transpose()?;
        let credit = columns
            .credit
            .and_then(|index| nonempty(record.get(index)))
            .map(|value| parse_amount_minor(value, &profile.formats))
            .transpose()?
            .map(checked_magnitude)
            .transpose()?;
        match (debit, credit) {
            (Some(_), Some(_)) => {
                return Err(invalid_money(
                    "debit + credit",
                    "both columns contain an amount",
                ));
            }
            (Some(value), None) => -value,
            (None, Some(value)) => value,
            (None, None) => return Err(invalid_money("", "both debit and credit are empty")),
        }
    };
    let balance_after_minor = columns
        .balance
        .and_then(|index| nonempty(record.get(index)))
        .map(|value| parse_amount_minor(value, &profile.formats))
        .transpose()?;
    let external_id = columns
        .transaction_id
        .and_then(|index| nonempty(record.get(index)))
        .map(ToOwned::to_owned);
    Ok(ParsedRow {
        booked_on,
        description_normalized: normalize_description(&description_raw),
        description_raw,
        amount_minor,
        balance_after_minor,
        external_id,
    })
}

#[derive(Debug)]
struct ResolvedColumns {
    date: usize,
    description: usize,
    amount: Option<usize>,
    debit: Option<usize>,
    credit: Option<usize>,
    balance: Option<usize>,
    transaction_id: Option<usize>,
}

impl ResolvedColumns {
    fn new(headers: &csv::StringRecord, mapping: &ColumnMapping) -> Result<Self, ImportError> {
        Ok(Self {
            date: required_header(headers, &mapping.date)?,
            description: required_header(headers, &mapping.description)?,
            amount: optional_header(headers, mapping.amount.as_deref())?,
            debit: optional_header(headers, mapping.debit.as_deref())?,
            credit: optional_header(headers, mapping.credit.as_deref())?,
            balance: optional_header(headers, mapping.balance.as_deref())?,
            transaction_id: optional_header(headers, mapping.transaction_id.as_deref())?,
        })
    }
}

fn required_header(headers: &csv::StringRecord, name: &str) -> Result<usize, ImportError> {
    headers
        .iter()
        .position(|header| header.trim().trim_start_matches('\u{feff}') == name.trim())
        .ok_or_else(|| ImportError::MissingColumn(name.to_owned()))
}

fn optional_header(
    headers: &csv::StringRecord,
    name: Option<&str>,
) -> Result<Option<usize>, ImportError> {
    name.map(|name| required_header(headers, name)).transpose()
}

fn raw_row_json(
    headers: &csv::StringRecord,
    record: &csv::StringRecord,
) -> Result<String, serde_json::Error> {
    let values: BTreeMap<&str, &str> = headers.iter().zip(record.iter()).collect();
    serde_json::to_string(&values)
}

fn dedup_key(account_key: &str, row: &ParsedRow) -> String {
    let identity = if let Some(external_id) = &row.external_id {
        format!("id|{account_key}|{external_id}")
    } else {
        format!(
            "fp|{}|{}|{}|{}|{}",
            account_key,
            row.booked_on,
            row.amount_minor,
            row.description_normalized,
            row.balance_after_minor
                .map(|value| value.to_string())
                .unwrap_or_else(|| "no-balance".to_owned())
        )
    };
    sha256_hex(identity.as_bytes())
}

fn normalize_description(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn existing_batch(
    connection: &Connection,
    account_id: i64,
    digest: &str,
    profile_digest: &str,
    profile_name: &str,
    account_key: &str,
) -> Result<Option<ImportReport>, rusqlite::Error> {
    let mut report = connection
        .query_row(
            "SELECT id, row_count, inserted_count, duplicate_count, error_count
             FROM import_batches
             WHERE account_id = ?1 AND content_sha256 = ?2 AND profile_sha256 = ?3",
            params![account_id, digest, profile_digest],
            |row| {
                Ok(ImportReport {
                    batch_id: row.get(0)?,
                    profile_name: profile_name.to_owned(),
                    account_key: account_key.to_owned(),
                    rows: row.get(1)?,
                    inserted: 0,
                    duplicates: row.get::<_, u64>(2)? + row.get::<_, u64>(3)?,
                    errors: row.get(4)?,
                    row_errors: Vec::new(),
                    exact_reimport: true,
                })
            },
        )
        .optional()?;
    if let Some(existing) = report.as_mut() {
        let mut statement = connection.prepare(
            "SELECT row_number, parse_error FROM raw_import_rows
             WHERE batch_id = ?1 AND parse_error IS NOT NULL
             ORDER BY row_number LIMIT 20",
        )?;
        existing.row_errors = statement
            .query_map([existing.batch_id], |row| {
                Ok(ImportRowError {
                    row_number: row.get(0)?,
                    message: row.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
    }
    Ok(report)
}

fn decode(bytes: &[u8], encoding_name: &str) -> Result<String, ImportError> {
    let encoding = if encoding_name.eq_ignore_ascii_case("utf-8")
        || encoding_name.eq_ignore_ascii_case("utf8")
    {
        UTF_8
    } else {
        Encoding::for_label(encoding_name.as_bytes())
            .ok_or_else(|| ImportError::UnknownEncoding(encoding_name.to_owned()))?
    };
    let (decoded, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return Err(ImportError::InvalidEncoding(encoding_name.to_owned()));
    }
    Ok(decoded.into_owned())
}

fn decode_for_preview(bytes: &[u8]) -> Result<(String, String), ImportError> {
    if let Ok(source) = std::str::from_utf8(bytes) {
        return Ok(("utf-8".to_owned(), source.to_owned()));
    }
    let encoding_name = "windows-1254";
    Ok((encoding_name.to_owned(), decode(bytes, encoding_name)?))
}

fn detect_header_row(source: &str) -> Result<(usize, u8), ImportError> {
    let mut best: Option<(u32, usize, u8)> = None;
    for (line_number, line) in source.lines().take(12).enumerate() {
        for delimiter in b";\t,|" {
            let fields: Vec<String> = line
                .split(char::from(*delimiter))
                .map(|field| field.trim().trim_matches('"').to_owned())
                .collect();
            if fields.len() < 2 {
                continue;
            }
            let suggestions = suggest_columns(&fields);
            let semantic_fields = u32::from(!suggestions.date.is_empty())
                + u32::from(!suggestions.description.is_empty())
                + u32::from(
                    suggestions.amount.is_some()
                        || suggestions.debit.is_some()
                        || suggestions.credit.is_some(),
                )
                + u32::from(suggestions.balance.is_some());
            let score = semantic_fields * 1_000 + fields.len() as u32;
            if best.is_none_or(|current| score > current.0) {
                best = Some((score, line_number, *delimiter));
            }
        }
    }
    best.map(|(_, line_number, delimiter)| (line_number, delimiter))
        .ok_or(ImportError::InvalidDelimiter)
}

fn resolve_delimiter(configured: &str, source: &str) -> Result<u8, ImportError> {
    if configured != "auto" {
        return configured
            .as_bytes()
            .first()
            .copied()
            .ok_or(ImportError::InvalidDelimiter);
    }
    let header = source
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    b";\t,|"
        .iter()
        .copied()
        .max_by_key(|candidate| header.bytes().filter(|byte| byte == candidate).count())
        .filter(|candidate| header.bytes().any(|byte| byte == *candidate))
        .ok_or(ImportError::InvalidDelimiter)
}

fn skip_leading_lines(source: &str, count: usize) -> &str {
    let mut remainder = source;
    for _ in 0..count {
        remainder = remainder
            .split_once('\n')
            .map(|(_, rest)| rest)
            .unwrap_or("");
    }
    remainder
}

fn checked_magnitude(value: i64) -> Result<i64, ImportError> {
    value
        .checked_abs()
        .ok_or_else(|| invalid_money(&value.to_string(), "amount magnitude overflow"))
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn invalid_money(value: &str, reason: &str) -> ImportError {
    ImportError::InvalidMoney {
        value: value.to_owned(),
        reason: reason.to_owned(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn default_delimiter() -> String {
    "auto".to_owned()
}

fn default_encoding() -> String {
    "utf-8".to_owned()
}

fn default_date_formats() -> Vec<String> {
    vec![
        "%d.%m.%Y".to_owned(),
        "%d/%m/%Y".to_owned(),
        "%Y-%m-%d".to_owned(),
    ]
}

fn default_decimal_separator() -> char {
    ','
}

fn default_thousands_separator() -> Option<char> {
    Some('.')
}

fn default_minor_unit_digits() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_profile() -> ImportProfile {
        parse_profile(include_str!(
            "../../../tests/fixtures/turkish-bank-profile.toml"
        ))
        .unwrap()
    }

    #[test]
    fn parses_turkish_money_without_floating_point() {
        let formats = ValueFormats::default();
        assert_eq!(parse_amount_minor("₺1.234,56", &formats).unwrap(), 123_456);
        assert_eq!(
            parse_amount_minor("-30.000,00 TL", &formats).unwrap(),
            -3_000_000
        );
        assert_eq!(parse_amount_minor("(42,10)", &formats).unwrap(), -4_210);
    }

    #[test]
    fn preview_suggests_turkish_mapping_and_formats_without_writing() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/turkish-bank.csv");
        let preview = preview_csv(path).unwrap();
        assert_eq!(preview.suggested_profile.delimiter, ";");
        assert_eq!(preview.suggested_profile.encoding, "utf-8");
        assert_eq!(preview.suggested_profile.columns.date, "Tarih");
        assert_eq!(preview.suggested_profile.columns.description, "Açıklama");
        assert_eq!(
            preview.suggested_profile.columns.amount.as_deref(),
            Some("İşlem Tutarı")
        );
        assert_eq!(preview.suggested_profile.formats.decimal_separator, ',');
        assert_eq!(preview.sample_rows.len(), 8);
    }

    #[test]
    fn saved_profile_shape_matches_later_statement_headers() {
        let profile = synthetic_profile();
        let preview = preview_csv(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/turkish-bank-overlap.csv"),
        )
        .unwrap();
        assert!(profile_matches_headers(&profile, &preview.headers));
    }

    #[test]
    fn exact_reimport_inserts_zero_transactions() {
        let mut db = runway_db::open_in_memory().unwrap();
        let profile = synthetic_profile();
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/turkish-bank.csv");
        let currency = CurrencyCode::new("TRY").unwrap();
        let first = import_csv(
            &mut db,
            &path,
            &profile,
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        let second = import_csv(
            &mut db,
            &path,
            &profile,
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        assert_eq!(first.inserted, 8);
        assert_eq!(first.errors, 0);
        assert_eq!(second.inserted, 0);
        assert_eq!(second.duplicates, 8);
        assert!(second.exact_reimport);
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM actual_transactions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 8);
    }

    #[test]
    fn overlapping_statement_rows_are_reconciled() {
        let mut db = runway_db::open_in_memory().unwrap();
        let profile = synthetic_profile();
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let currency = CurrencyCode::new("TRY").unwrap();
        import_csv(
            &mut db,
            fixture_root.join("turkish-bank.csv"),
            &profile,
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        let overlap = import_csv(
            &mut db,
            fixture_root.join("turkish-bank-overlap.csv"),
            &profile,
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        assert_eq!(overlap.inserted, 1);
        assert_eq!(overlap.duplicates, 2);
    }

    #[test]
    fn overlapping_rows_reconcile_without_bank_transaction_ids() {
        let mut db = runway_db::open_in_memory().unwrap();
        let mut profile = synthetic_profile();
        profile.columns.transaction_id = None;
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let currency = CurrencyCode::new("TRY").unwrap();
        import_csv(
            &mut db,
            fixture_root.join("turkish-bank.csv"),
            &profile,
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        let overlap = import_csv(
            &mut db,
            fixture_root.join("turkish-bank-overlap.csv"),
            &profile,
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        assert_eq!(overlap.inserted, 1);
        assert_eq!(overlap.duplicates, 2);
    }

    #[test]
    fn corrected_profile_retries_the_same_file_without_duplicates() {
        let mut db = runway_db::open_in_memory().unwrap();
        let mut incorrect = synthetic_profile();
        incorrect.formats.date = vec!["%Y/%m/%d".to_owned()];
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/turkish-bank.csv");
        let currency = CurrencyCode::new("TRY").unwrap();
        let failed = import_csv(
            &mut db,
            &path,
            &incorrect,
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        assert_eq!(failed.inserted, 0);
        assert_eq!(failed.errors, 8);
        assert_eq!(failed.row_errors.len(), 8);

        let corrected = import_csv(
            &mut db,
            &path,
            &synthetic_profile(),
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        assert_eq!(corrected.inserted, 8);
        assert_eq!(corrected.errors, 0);
        assert!(!corrected.exact_reimport);

        let exact = import_csv(
            &mut db,
            &path,
            &synthetic_profile(),
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        assert!(exact.exact_reimport);
        assert_eq!(exact.inserted, 0);
    }

    #[test]
    fn import_to_explainable_runway_and_persistent_correction() {
        let mut db = runway_db::open_in_memory().unwrap();
        let profile = synthetic_profile();
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let currency = CurrencyCode::new("TRY").unwrap();
        import_csv(
            &mut db,
            fixture_root.join("turkish-bank.csv"),
            &profile,
            "checking-try",
            "Synthetic checking",
            &currency,
        )
        .unwrap();
        runway_db::set_scenario(&db, "no-work", &currency, 2_000_000, None).unwrap();
        runway_db::add_forecast_rule(
            &db,
            "no-work",
            &runway_db::NewForecastRule {
                label: "Scholarship".to_owned(),
                direction: runway_core::CashFlowDirection::Income,
                amount_minor: 650_000,
                currency: currency.clone(),
                cadence: runway_core::ForecastCadence::Monthly { day_of_month: 1 },
                starts_on: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
                ends_on: Some(NaiveDate::from_ymd_opt(2027, 6, 30).unwrap()),
                evidence: vec!["transaction:4".to_owned(), "transaction:7".to_owned()],
            },
        )
        .unwrap();

        let (_, first_input) = runway_db::build_runway_input(&db, "no-work", None).unwrap();
        let first_result = runway_core::calculate_runway(&first_input).unwrap();
        assert_eq!(first_result.zero_date, NaiveDate::from_ymd_opt(2026, 9, 12));
        assert_eq!(first_input.historical_burn.included_transaction_count, 5);

        runway_db::annotate_transaction(
            &db,
            6,
            "exceptional",
            Some("Synthetic test: explicitly non-repeating"),
        )
        .unwrap();
        let (_, corrected_input) = runway_db::build_runway_input(&db, "no-work", None).unwrap();
        let corrected_result = runway_core::calculate_runway(&corrected_input).unwrap();
        assert_eq!(
            corrected_result.zero_date,
            NaiveDate::from_ymd_opt(2026, 12, 16)
        );
        assert_eq!(
            corrected_input.historical_burn.included_transaction_count,
            4
        );

        let raw_rows: i64 = db
            .query_row("SELECT COUNT(*) FROM raw_import_rows", [], |row| row.get(0))
            .unwrap();
        let forecast_rows: i64 = db
            .query_row("SELECT COUNT(*) FROM forecast_rules", [], |row| row.get(0))
            .unwrap();
        assert_eq!(raw_rows, 8);
        assert_eq!(forecast_rows, 1);
    }

    #[test]
    fn skip_rows_applies_before_the_csv_header() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            temp.path(),
            "Generated by Synthetic Bank\nTarih;Açıklama;İşlem Tutarı;Bakiye;İşlem No\n01.08.2026;MARKET;-10,00;990,00;P-1\n",
        )
        .unwrap();
        let mut profile = synthetic_profile();
        profile.skip_rows = 1;
        let mut db = runway_db::open_in_memory().unwrap();
        let report = import_csv(
            &mut db,
            temp.path(),
            &profile,
            "checking-try",
            "Synthetic checking",
            &CurrencyCode::new("TRY").unwrap(),
        )
        .unwrap();
        assert_eq!(report.inserted, 1);
        assert_eq!(report.errors, 0);
    }
}
