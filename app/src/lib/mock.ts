import type { DashboardResponse } from './types';

/** Synthetic UI fixture. Enabled only with VITE_MOCK_DATA=1. */
export const mockDashboard: DashboardResponse = {
  summary: {
    transaction_count: 428,
    last_actual_data: '2026-08-20',
    accounts: [
      {
        external_key: 'synthetic-checking',
        display_name: 'Main checking',
        currency: 'TRY',
        transaction_count: 428,
        last_actual_data: '2026-08-20',
      },
    ],
    scenarios: ['no-work'],
    unresolved_outflow_count: 3,
  },
  scenario: {
    id: 1,
    name: 'no-work',
    currency: 'TRY',
    reserve_minor: 2_000_000,
    explicit_assets_minor: 13_200_000,
    assets_as_of: '2026-08-20',
    max_horizon_days: 36_525,
  },
  forecastRules: [
    {
      id: 1,
      label: 'Scholarship',
      direction: 'income',
      amount_minor: 650_000,
      currency: 'TRY',
      cadence: 'monthly',
      day_of_month: 1,
      starts_on: '2026-09-01',
      ends_on: '2027-06-30',
      source: 'user_confirmed',
      evidence: [],
    },
  ],
  runway: {
    input: {
      as_of: '2026-08-20',
      liquid_assets: { amount_minor: 13_200_000, currency: 'TRY' },
      reserve: { amount_minor: 2_000_000, currency: 'TRY' },
      historical_burn: {
        expense_minor: 9_184_000,
        observed_days: 181,
        observed_from: '2026-02-21',
        observed_through: '2026-08-20',
        included_transaction_count: 302,
        excluded_transaction_count: 126,
      },
      forecast_rules: [],
      max_horizon_days: 36_525,
    },
    result: {
      as_of: '2026-08-20',
      currency: 'TRY',
      liquid_assets_minor: 13_200_000,
      reserve_minor: 2_000_000,
      zero_date: '2027-11-01',
      runway_days: 438,
      display_duration: '14 months 12 days',
      projected_balance_minor: 1_996_000,
      historical_expense_applied_minor: 17_700_000,
      rule_contributions: [
        {
          rule_id: 'forecast_rule:1',
          label: 'Scholarship',
          direction: 'income',
          occurrence_count: 10,
          amount_minor: 6_500_000,
        },
      ],
      last_actual_data: '2026-08-20',
      horizon_days: 36_525,
      warnings: [],
    },
  },
  runwayError: null,
  reviewCandidates: [
    {
      transaction: {
        id: 391,
        booked_on: '2026-07-14',
        account: 'Main checking',
        description: 'EFT TRANSFER 784102',
        amount_minor: -2_500_000,
        currency: 'TRY',
        balance_after_minor: 8_420_000,
        interpretation: null,
      },
      estimatedEffectDays: 49,
    },
  ],
};
