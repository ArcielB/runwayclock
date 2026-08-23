import type { Language } from './i18n';

export function formatMoney(minor: number, currency: string, language: Language): string {
  return new Intl.NumberFormat(language === 'tr' ? 'tr-TR' : 'en-US', {
    style: 'currency',
    currency,
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  }).format(minor / 100);
}

export function formatDate(value: string | null, language: Language): string {
  if (!value) return '—';
  const [year, month, day] = value.split('-').map(Number);
  return new Intl.DateTimeFormat(language === 'tr' ? 'tr-TR' : 'en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  }).format(new Date(Date.UTC(year, month - 1, day)));
}

export function formatDuration(duration: string | null, language: Language): string {
  if (!duration || language === 'en') return duration || '—';
  return duration
    .replace(/months?/g, 'ay')
    .replace(/days?/g, 'gün');
}

export function todayIso(): string {
  const now = new Date();
  const local = new Date(now.getTime() - now.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 10);
}
