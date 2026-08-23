<script lang="ts">
  import type { Labels, Language } from '../lib/i18n';
  import type { ReviewCandidate } from '../lib/types';
  import { annotateTransaction, errorMessage } from '../lib/api';
  import { formatDate, formatMoney } from '../lib/format';

  export let candidates: ReviewCandidate[];
  export let unresolvedCount: number;
  export let labels: Labels;
  export let language: Language;
  export let onChanged: () => void;

  let busyId: number | null = null;
  let error = '';

  async function classify(candidate: ReviewCandidate, classification: string) {
    busyId = candidate.transaction.id;
    error = '';
    try {
      await annotateTransaction(candidate.transaction.id, classification);
      await onChanged();
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      busyId = null;
    }
  }
</script>

<section class="page-heading">
  <div>
    <span class="eyebrow">{unresolvedCount} {labels.needsAttention.toLowerCase()}</span>
    <h1>{labels.review}</h1>
    <p>{labels.reviewPrompt}</p>
  </div>
</section>

{#if error}<div class="alert error">{error}</div>{/if}

{#if candidates.length === 0}
  <div class="empty-state card"><span>✓</span><h2>{labels.dataFresh}</h2><p>{labels.reviewed}</p></div>
{:else}
  <div class="review-list">
    {#each candidates as candidate}
      <article class="review-card card">
        <div class="transaction-main">
          <span class="transaction-date">{formatDate(candidate.transaction.booked_on, language)}</span>
          <h3>{candidate.transaction.description}</h3>
          <small>{candidate.transaction.account}</small>
        </div>
        <div class="transaction-impact">
          <strong>{formatMoney(candidate.transaction.amount_minor, candidate.transaction.currency, language)}</strong>
          {#if candidate.estimatedEffectDays}<small>{labels.couldChange} ~{candidate.estimatedEffectDays} {labels.days}</small>{/if}
        </div>
        <div class="review-actions">
          <button disabled={busyId === candidate.transaction.id} onclick={() => classify(candidate, 'transfer')}>{labels.ownTransfer}</button>
          <button disabled={busyId === candidate.transaction.id} onclick={() => classify(candidate, 'exceptional')}>{labels.exceptional}</button>
          <button disabled={busyId === candidate.transaction.id} onclick={() => classify(candidate, 'variable_recurrent')}>{labels.ongoing}</button>
        </div>
      </article>
    {/each}
  </div>
{/if}
