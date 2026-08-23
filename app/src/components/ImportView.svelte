<script lang="ts">
  import type { Labels } from '../lib/i18n';
  import type { ImportProfile, ImportReport, PreviewResponse } from '../lib/types';
  import {
    chooseStatement,
    errorMessage,
    importStatement,
    previewStatement,
  } from '../lib/api';

  export let labels: Labels;
  export let onImported: (report: ImportReport) => void;

  let path = '';
  let draft: PreviewResponse | null = null;
  let busy = false;
  let error = '';
  let accountMode = '__new__';
  let dateFormat = '%d.%m.%Y';

  async function selectFile() {
    error = '';
    const selected = await chooseStatement();
    if (!selected) return;
    path = selected;
    busy = true;
    try {
      draft = await previewStatement(path);
      accountMode = draft.accounts.some((account) => account.external_key === draft?.accountKey)
        ? draft.accountKey
        : '__new__';
      dateFormat = draft.profile.formats.date[0] || '%d.%m.%Y';
    } catch (reason) {
      error = errorMessage(reason);
      draft = null;
    } finally {
      busy = false;
    }
  }

  function selectAccount(event: Event) {
    if (!draft) return;
    accountMode = (event.currentTarget as HTMLSelectElement).value;
    if (accountMode === '__new__') {
      draft.accountKey = '';
      draft.accountName = '';
      return;
    }
    const account = draft.accounts.find((candidate) => candidate.external_key === accountMode);
    if (account) {
      draft.accountKey = account.external_key;
      draft.accountName = account.display_name;
      draft.currency = account.currency;
    }
  }

  function selectDecimal(event: Event) {
    if (!draft) return;
    const separator = (event.currentTarget as HTMLSelectElement).value;
    draft.profile.formats.decimal_separator = separator;
    draft.profile.formats.thousands_separator = separator === ',' ? '.' : ',';
  }

  function normalizedProfile(profile: ImportProfile): ImportProfile {
    const optional = (value: string | null) => value && value.trim() ? value : null;
    const amount = optional(profile.columns.amount);
    return {
      ...profile,
      formats: {
        ...profile.formats,
        date: [dateFormat],
      },
      columns: {
        ...profile.columns,
        amount,
        debit: amount ? null : optional(profile.columns.debit),
        credit: amount ? null : optional(profile.columns.credit),
        balance: optional(profile.columns.balance),
        transaction_id: optional(profile.columns.transaction_id),
      },
    };
  }

  async function runImport() {
    if (!draft) return;
    error = '';
    busy = true;
    try {
      const report = await importStatement({
        path,
        profile: normalizedProfile(draft.profile),
        accountKey: draft.accountKey,
        accountName: draft.accountName,
        currency: draft.currency.toUpperCase(),
      });
      onImported(report);
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      busy = false;
    }
  }
</script>

<section class="page-heading">
  <div>
    <span class="eyebrow">CSV → SQLite</span>
    <h1>{draft ? labels.previewTitle : labels.import}</h1>
    <p>{labels.startPrompt}</p>
  </div>
  {#if draft}
    <button class="secondary" onclick={selectFile}>{labels.chooseAnother}</button>
  {/if}
</section>

{#if error}<div class="alert error">{error}</div>{/if}

{#if !draft}
  <button class="file-drop" onclick={selectFile} disabled={busy}>
    <span class="upload-icon">↥</span>
    <strong>{busy ? labels.loading : labels.chooseCsv}</strong>
    <small>.csv · UTF-8 · Windows-1254</small>
  </button>
{:else}
  <div class="import-layout">
    <div class="import-main">
      <article class="card statement-card">
        <div class="statement-status" class:recognized={draft.matchedSavedProfile}>
          <i></i>
          <div>
            <strong>{draft.sourceName}</strong>
            <small>{draft.matchedSavedProfile ? labels.recognized : labels.newFormat}</small>
          </div>
        </div>
        {#if draft.accounts.length > 0}
          <label class="account-picker">{labels.importInto}
            <select value={accountMode} onchange={selectAccount}>
              {#each draft.accounts as account}<option value={account.external_key}>{account.display_name} · {account.currency}</option>{/each}
              <option value="__new__">＋ {labels.newAccount}</option>
            </select>
          </label>
        {/if}
        <div class="form-grid two">
          <label>{labels.accountName}<input bind:value={draft.accountName} /></label>
          <label>{labels.accountKey}<input bind:value={draft.accountKey} /></label>
          <label>{labels.currency}<input maxlength="3" bind:value={draft.currency} /></label>
          <label>{labels.profileName}<input bind:value={draft.profile.name} /></label>
        </div>
      </article>

      <article class="card">
        <div class="section-heading"><div><span class="eyebrow">2 / 3</span><h2>{labels.columnMapping}</h2></div></div>
        <div class="mapping-grid">
          <label>{labels.date}<select bind:value={draft.profile.columns.date}>{#each draft.headers as header}<option value={header}>{header}</option>{/each}</select></label>
          <label>{labels.description}<select bind:value={draft.profile.columns.description}>{#each draft.headers as header}<option value={header}>{header}</option>{/each}</select></label>
          <label>{labels.amount}<select bind:value={draft.profile.columns.amount}><option value="">{labels.notPresent}</option>{#each draft.headers as header}<option value={header}>{header}</option>{/each}</select></label>
          <label>{labels.balance}<select bind:value={draft.profile.columns.balance}><option value="">{labels.notPresent}</option>{#each draft.headers as header}<option value={header}>{header}</option>{/each}</select></label>
          <label>{labels.debit}<select bind:value={draft.profile.columns.debit}><option value="">{labels.notPresent}</option>{#each draft.headers as header}<option value={header}>{header}</option>{/each}</select></label>
          <label>{labels.credit}<select bind:value={draft.profile.columns.credit}><option value="">{labels.notPresent}</option>{#each draft.headers as header}<option value={header}>{header}</option>{/each}</select></label>
          <label>{labels.transactionId}<select bind:value={draft.profile.columns.transaction_id}><option value="">{labels.notPresent}</option>{#each draft.headers as header}<option value={header}>{header}</option>{/each}</select></label>
          <label>{labels.decimalSeparator}<select value={draft.profile.formats.decimal_separator} onchange={selectDecimal}><option value=",">Comma · 1.234,56</option><option value=".">Point · 1,234.56</option></select></label>
          <label>{labels.dateFormat}<select bind:value={dateFormat}><option value="%d.%m.%Y">DD.MM.YYYY</option><option value="%d/%m/%Y">DD/MM/YYYY</option><option value="%Y-%m-%d">YYYY-MM-DD</option><option value="%m/%d/%Y">MM/DD/YYYY</option></select></label>
          <label>{labels.encoding}<select bind:value={draft.profile.encoding}><option value="utf-8">UTF-8</option><option value="windows-1254">Windows-1254</option></select></label>
          <label>{labels.delimiter}<select bind:value={draft.profile.delimiter}><option value=";">Semicolon ;</option><option value=",">Comma ,</option><option value={'\t'}>Tab</option><option value="|">Pipe |</option></select></label>
          <label>{labels.skipRows}<input type="number" min="0" max="50" bind:value={draft.profile.skip_rows} /></label>
        </div>
      </article>

      <article class="card preview-card">
        <div class="section-heading"><div><span class="eyebrow">{labels.rawPreview}</span><h2>{draft.sourceName}</h2></div></div>
        <div class="table-scroll">
          <table>
            <thead><tr>{#each draft.headers as header}<th>{header}</th>{/each}</tr></thead>
            <tbody>{#each draft.sampleRows as row}<tr>{#each draft.headers as _, index}<td>{row[index] || ''}</td>{/each}</tr>{/each}</tbody>
          </table>
        </div>
      </article>
    </div>

    <aside class="import-summary card">
      <span class="eyebrow">3 / 3</span>
      <h2>{labels.importNow}</h2>
      <p>Raw rows will be preserved. Existing transaction IDs and overlapping fingerprints will be reconciled before anything is added.</p>
      <dl>
        <div><dt>{labels.sourceFile}</dt><dd>{draft.sourceName}</dd></div>
        <div><dt>{labels.account}</dt><dd>{draft.accountName}</dd></div>
        <div><dt>{labels.currency}</dt><dd>{draft.currency.toUpperCase()}</dd></div>
      </dl>
      <button class="primary full" onclick={runImport} disabled={busy || !draft.profile.columns.date || !draft.profile.columns.description}>
        {busy ? labels.importing : labels.importNow}
      </button>
      <small class="privacy-note">⌁ {labels.noCloud}</small>
    </aside>
  </div>
{/if}
