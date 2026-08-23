import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type {
  DashboardResponse,
  ImportProfile,
  ImportReport,
  PreviewResponse,
  Scenario,
} from './types';
import { mockDashboard } from './mock';

const mockMode = import.meta.env.VITE_MOCK_DATA === '1';

export async function chooseStatement(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    title: 'Choose a bank statement',
    filters: [{ name: 'Bank statement', extensions: ['csv', 'txt'] }],
  });
  return typeof selected === 'string' ? selected : null;
}

export function getDashboard(): Promise<DashboardResponse> {
  if (mockMode) return Promise.resolve(structuredClone(mockDashboard));
  return invoke('get_dashboard');
}

export function previewStatement(path: string): Promise<PreviewResponse> {
  return invoke('preview_statement', { request: { path } });
}

export function importStatement(request: {
  path: string;
  profile: ImportProfile;
  accountKey: string;
  accountName: string;
  currency: string;
}): Promise<ImportReport> {
  return invoke('import_statement', { request });
}

export function saveScenario(request: {
  name: string;
  currency: string;
  reserve: string;
  assets: string | null;
  assetsAsOf: string | null;
}): Promise<Scenario> {
  return invoke('save_scenario', { request });
}

export function addFlow(request: {
  scenario: string;
  label: string;
  direction: 'income' | 'expense';
  amount: string;
  currency: string;
  cadence: 'once' | 'monthly';
  startsOn: string;
  endsOn: string | null;
  dayOfMonth: number | null;
  evidence: string[];
}): Promise<number> {
  return invoke('add_flow', { request });
}

export function removeFlow(scenario: string, ruleId: number): Promise<boolean> {
  return invoke('remove_flow', { scenario, ruleId });
}

export function annotateTransaction(
  transactionId: number,
  classification: string,
  note?: string,
): Promise<void> {
  return invoke('annotate_transaction', {
    request: { transactionId, class: classification, note: note || null },
  });
}

export function errorMessage(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return 'Something went wrong.';
}
