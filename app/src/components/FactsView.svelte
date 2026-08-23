<script lang="ts">
  import type { Labels, Language } from '../lib/i18n';
  import type { ForecastRuleListItem, Scenario } from '../lib/types';
  import { addFlow, errorMessage, removeFlow, saveScenario } from '../lib/api';
  import { formatDate, formatMoney, todayIso } from '../lib/format';

  export let scenario: Scenario | null;
  export let rules: ForecastRuleListItem[];
  export let labels: Labels;
  export let language: Language;
  export let onChanged: () => void;

  let currency = scenario?.currency || 'TRY';
  let reserve = scenario ? (scenario.reserve_minor / 100).toFixed(2) : '0';
  let useImported = scenario?.explicit_assets_minor == null;
  let assets = scenario?.explicit_assets_minor != null ? (scenario.explicit_assets_minor / 100).toFixed(2) : '';
  let assetsAsOf = scenario?.assets_as_of || todayIso();
  let settingsBusy = false;
  let flowBusy = false;
  let error = '';
  let notice = '';

  let label = '';
  let direction: 'income' | 'expense' = 'income';
  let amount = '';
  let cadence: 'once' | 'monthly' = 'monthly';
  let startsOn = todayIso();
  let endsOn = '';
  let dayOfMonth = 1;

  async function saveSettings() {
    settingsBusy = true;
    error = '';
    notice = '';
    try {
      await saveScenario({
        name: 'no-work',
        currency: currency.toUpperCase(),
        reserve,
        assets: useImported ? null : assets,
        assetsAsOf: useImported ? null : assetsAsOf,
      });
      notice = labels.userConfirmed;
      await onChanged();
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      settingsBusy = false;
    }
  }

  async function saveFlow() {
    flowBusy = true;
    error = '';
    notice = '';
    try {
      await addFlow({
        scenario: 'no-work',
        label,
        direction,
        amount,
        currency: currency.toUpperCase(),
        cadence,
        startsOn,
        endsOn: endsOn || null,
        dayOfMonth: cadence === 'monthly' ? dayOfMonth : null,
        evidence: [],
      });
      label = '';
      amount = '';
      notice = labels.userConfirmed;
      await onChanged();
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      flowBusy = false;
    }
  }

  async function deleteRule(id: number) {
    try {
      await removeFlow('no-work', id);
      await onChanged();
    } catch (reason) {
      error = errorMessage(reason);
    }
  }
</script>

<section class="page-heading">
  <div>
    <span class="eyebrow">{labels.noWork}</span>
    <h1>{labels.facts}</h1>
    <p>{labels.scenarioHelp}</p>
  </div>
</section>

{#if error}<div class="alert error">{error}</div>{/if}
{#if notice}<div class="alert success">✓ {notice}</div>{/if}

<div class="facts-layout">
  <article class="card">
    <div class="section-heading"><div><span class="eyebrow">01</span><h2>{labels.scenarioSettings}</h2></div></div>
    <div class="form-grid two">
      <label>{labels.currency}<input maxlength="3" bind:value={currency} /></label>
      <label>{labels.reserve}<input inputmode="decimal" bind:value={reserve} placeholder="20000" /></label>
    </div>
    <label class="check-row"><input type="checkbox" bind:checked={useImported} /><span><strong>{labels.useImportedBalance}</strong><small>{labels.actualThrough}: {scenario?.assets_as_of || 'latest import'}</small></span></label>
    {#if !useImported}
      <div class="form-grid two inset-fields">
        <label>{labels.currentAssets}<input inputmode="decimal" bind:value={assets} placeholder="132000" /></label>
        <label>{labels.assetsAsOf}<input type="date" bind:value={assetsAsOf} /></label>
      </div>
    {/if}
    <button class="primary" onclick={saveSettings} disabled={settingsBusy}>{labels.saveSettings}</button>
  </article>

  <article class="card flow-form-card">
    <div class="section-heading"><div><span class="eyebrow">02</span><h2>{labels.addFact}</h2></div></div>
    <div class="form-grid two">
      <label>{labels.label}<input bind:value={label} placeholder="Scholarship" /></label>
      <label>{labels.amount}<input inputmode="decimal" bind:value={amount} placeholder="6500" /></label>
      <label>{labels.direction}<select bind:value={direction}><option value="income">{labels.income}</option><option value="expense">{labels.expense}</option></select></label>
      <label>{labels.cadence}<select bind:value={cadence}><option value="monthly">{labels.monthly}</option><option value="once">{labels.once}</option></select></label>
      <label>{labels.startsOn}<input type="date" bind:value={startsOn} /></label>
      {#if cadence === 'monthly'}
        <label>{labels.endsOn}<input type="date" bind:value={endsOn} /></label>
        <label>{labels.dayOfMonth}<input type="number" min="1" max="31" bind:value={dayOfMonth} /></label>
      {/if}
    </div>
    <button class="primary" onclick={saveFlow} disabled={flowBusy || !label || !amount}>{labels.saveFact}</button>
  </article>
</div>

<section class="rules-section">
  <div class="section-heading"><div><span class="eyebrow">{labels.userConfirmed}</span><h2>{labels.futureFlows}</h2></div></div>
  {#if rules.length === 0}
    <div class="empty-inline">{labels.noFlows}</div>
  {:else}
    <div class="rule-list">
      {#each rules as rule}
        <article class="rule-card">
          <span class:income={rule.direction === 'income'} class="direction-mark">{rule.direction === 'income' ? '+' : '−'}</span>
          <div><strong>{rule.label}</strong><small>{rule.cadence === 'monthly' ? labels.monthly : labels.once} · {formatDate(rule.starts_on, language)}{rule.ends_on ? ` → ${formatDate(rule.ends_on, language)}` : ''}</small></div>
          <strong>{formatMoney(rule.amount_minor, rule.currency, language)}</strong>
          <button class="icon-button" title={labels.remove} onclick={() => deleteRule(rule.id)}>×</button>
        </article>
      {/each}
    </div>
  {/if}
</section>
