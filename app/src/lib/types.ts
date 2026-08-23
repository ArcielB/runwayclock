export type Page = 'dashboard' | 'import' | 'review' | 'facts';

export interface CurrencyCode {
  // CurrencyCode serializes as a string; this alias is kept for documentation.
}

export interface Money {
  amount_minor: number;
  currency: string;
}

export interface HistoricalBurn {
  expense_minor: number;
  observed_days: number;
  observed_from: string;
  observed_through: string;
  included_transaction_count: number;
  excluded_transaction_count: number;
}

export interface ForecastRule {
  id: string;
  label: string;
  direction: 'income' | 'expense';
  amount_minor: number;
  currency: string;
  cadence: { kind: 'once' } | { kind: 'monthly'; day_of_month: number };
  starts_on: string;
  ends_on: string | null;
  provenance: string;
  confidence_ppm: number;
  evidence: string[];
}

export interface RunwayInput {
  as_of: string;
  liquid_assets: Money;
  reserve: Money;
  historical_burn: HistoricalBurn;
  forecast_rules: ForecastRule[];
  max_horizon_days: number;
}

export interface RuleContribution {
  rule_id: string;
  label: string;
  direction: 'income' | 'expense';
  occurrence_count: number;
  amount_minor: number;
}

export interface RunwayResult {
  as_of: string;
  currency: string;
  liquid_assets_minor: number;
  reserve_minor: number;
  zero_date: string | null;
  runway_days: number | null;
  display_duration: string | null;
  projected_balance_minor: number;
  historical_expense_applied_minor: number;
  rule_contributions: RuleContribution[];
  last_actual_data: string;
  horizon_days: number;
  warnings: string[];
}

export interface Scenario {
  id: number;
  name: string;
  currency: string;
  reserve_minor: number;
  explicit_assets_minor: number | null;
  assets_as_of: string | null;
  max_horizon_days: number;
}

export interface ForecastRuleListItem {
  id: number;
  label: string;
  direction: 'income' | 'expense';
  amount_minor: number;
  currency: string;
  cadence: 'once' | 'monthly';
  day_of_month: number | null;
  starts_on: string;
  ends_on: string | null;
  source: string;
  evidence: string[];
}

export interface TransactionItem {
  id: number;
  booked_on: string;
  account: string;
  description: string;
  amount_minor: number;
  currency: string;
  balance_after_minor: number | null;
  interpretation: string | null;
}

export interface ReviewCandidate {
  transaction: TransactionItem;
  estimatedEffectDays: number | null;
}

export interface AccountSummary {
  external_key: string;
  display_name: string;
  currency: string;
  transaction_count: number;
  last_actual_data: string | null;
}

export interface AppSummary {
  transaction_count: number;
  last_actual_data: string | null;
  accounts: AccountSummary[];
  scenarios: string[];
  unresolved_outflow_count: number;
}

export interface DashboardResponse {
  summary: AppSummary;
  scenario: Scenario | null;
  forecastRules: ForecastRuleListItem[];
  runway: { input: RunwayInput; result: RunwayResult } | null;
  runwayError: string | null;
  reviewCandidates: ReviewCandidate[];
}

export interface ColumnMapping {
  date: string;
  description: string;
  amount: string | null;
  debit: string | null;
  credit: string | null;
  balance: string | null;
  transaction_id: string | null;
}

export interface ValueFormats {
  date: string[];
  decimal_separator: string;
  thousands_separator: string | null;
  minor_unit_digits: number;
}

export interface ImportProfile {
  name: string;
  delimiter: string;
  encoding: string;
  skip_rows: number;
  columns: ColumnMapping;
  formats: ValueFormats;
}

export interface PreviewResponse {
  sourceName: string;
  headers: string[];
  sampleRows: string[][];
  profile: ImportProfile;
  matchedSavedProfile: boolean;
  accountKey: string;
  accountName: string;
  currency: string;
  accounts: AccountSummary[];
}

export interface ImportReport {
  batch_id: number;
  profile_name: string;
  account_key: string;
  rows: number;
  inserted: number;
  duplicates: number;
  errors: number;
  row_errors: Array<{ row_number: number; message: string }>;
  exact_reimport: boolean;
}
