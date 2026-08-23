<script lang="ts">
  import type { Labels, Language } from '../lib/i18n';
  import type { DashboardResponse, Page } from '../lib/types';
  import { formatDate, formatDuration, formatMoney } from '../lib/format';

  export let data: DashboardResponse;
  export let labels: Labels;
  export let language: Language;
  export let onNavigate: (page: Page) => void;

  $: runway = data.runway;
  $: input = runway?.input;
  $: result = runway?.result;
  $: monthlySpend = input
    ? Math.trunc((input.historical_burn.expense_minor * 30) / input.historical_burn.observed_days)
    : 0;
</script>

{#if data.summary.transaction_count === 0}
  <section class="onboarding surface-grid">
    <div class="onboarding-copy">
      <span class="eyebrow">{labels.noWork}</span>
      <h1>{labels.importFirst}</h1>
      <p>{labels.startPrompt}</p>
      <button class="primary large" onclick={() => onNavigate('import')}>{labels.chooseCsv}</button>
      <div class="steps">{labels.setupSteps}</div>
    </div>
    <div class="empty-clock" aria-hidden="true">
      <div class="clock-face"><span></span></div>
      <div class="empty-number">—</div>
      <small>{labels.runway}</small>
    </div>
  </section>
{:else}
  <section class="dashboard-grid">
    <article class="runway-hero surface-grid">
      <div class="hero-topline">
        <span class="eyebrow">{labels.noWork}</span>
        <span class="freshness"><i></i>{labels.actualThrough} {formatDate(data.summary.last_actual_data, language)}</span>
      </div>
      {#if result}
        <div class="runway-value">
          <span>{labels.runway}</span>
          <strong>{result.display_duration ? formatDuration(result.display_duration, language) : labels.beyondHorizon}</strong>
          <small>{labels.reserveReached}: {formatDate(result.zero_date, language)}</small>
        </div>
      {:else}
        <div class="runway-value compact">
          <span>{labels.runway}</span>
          <strong>—</strong>
          <small>{data.runwayError || labels.warning}</small>
        </div>
      {/if}
      <button class="secondary hero-action" onclick={() => onNavigate('import')}>{labels.importNew}</button>
    </article>

    <div class="metrics">
      <article class="metric-card">
        <span>{labels.liquidAssets}</span>
        <strong>{input ? formatMoney(input.liquid_assets.amount_minor, input.liquid_assets.currency, language) : '—'}</strong>
        <small>{data.summary.accounts.length} {labels.account.toLowerCase()}</small>
      </article>
      <article class="metric-card">
        <span>{labels.expectedSpending}</span>
        <strong>{input ? formatMoney(monthlySpend, input.liquid_assets.currency, language) : '—'}</strong>
        <small>{input?.historical_burn.observed_days || 0} {labels.days} history</small>
      </article>
      <article class="metric-card">
        <span>{labels.reserve}</span>
        <strong>{input ? formatMoney(input.reserve.amount_minor, input.reserve.currency, language) : '—'}</strong>
        <button class="text-button" onclick={() => onNavigate('facts')}>{labels.saveSettings} →</button>
      </article>
    </div>

    <article class="explanation card">
      <div class="section-heading">
        <div>
          <span class="eyebrow">{labels.why}</span>
          <h2>{labels.baseline}</h2>
        </div>
        <span class="confidence">{input && input.historical_burn.observed_days >= 180 ? 'High' : input && input.historical_burn.observed_days >= 30 ? 'Medium' : 'Low'} history coverage</span>
      </div>
      {#if input && result}
        <div class="formula-row">
          <div class="formula-item positive">
            <span>+</span>
            <div><strong>{formatMoney(input.liquid_assets.amount_minor, input.liquid_assets.currency, language)}</strong><small>{labels.liquidAssets}</small></div>
          </div>
          {#each result.rule_contributions as contribution}
            <div class:positive={contribution.amount_minor > 0} class:negative={contribution.amount_minor < 0} class="formula-item">
              <span>{contribution.amount_minor > 0 ? '+' : '−'}</span>
              <div><strong>{formatMoney(Math.abs(contribution.amount_minor), input.liquid_assets.currency, language)}</strong><small>{contribution.label} · {contribution.occurrence_count}×</small></div>
            </div>
          {/each}
          <div class="formula-item negative">
            <span>−</span>
            <div><strong>{formatMoney(result.historical_expense_applied_minor, input.liquid_assets.currency, language)}</strong><small>{labels.baseline}</small></div>
          </div>
          <div class="formula-item reserve">
            <span>⌁</span>
            <div><strong>{formatMoney(input.reserve.amount_minor, input.reserve.currency, language)}</strong><small>{labels.reserve}</small></div>
          </div>
        </div>
        <p class="evidence-note">
          {input.historical_burn.included_transaction_count} {labels.includedHistory}
          {input.historical_burn.observed_days} {labels.days}. Forecast facts stay separate from imported transactions.
        </p>
      {/if}
    </article>

    {#if data.reviewCandidates.length > 0}
      <article class="attention-card">
        <div class="attention-count">{data.summary.unresolved_outflow_count}</div>
        <div>
          <span class="eyebrow">{labels.needsAttention}</span>
          <h3>{data.reviewCandidates[0].transaction.description}</h3>
          <p>{labels.reviewPrompt}</p>
        </div>
        <button class="secondary" onclick={() => onNavigate('review')}>{labels.review} →</button>
      </article>
    {/if}
  </section>
{/if}
