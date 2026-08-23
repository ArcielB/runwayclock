<script lang="ts">
  import { onMount } from 'svelte';
  import DashboardView from './components/DashboardView.svelte';
  import ImportView from './components/ImportView.svelte';
  import ReviewView from './components/ReviewView.svelte';
  import FactsView from './components/FactsView.svelte';
  import { errorMessage, getDashboard } from './lib/api';
  import { translations, type Language } from './lib/i18n';
  import type { DashboardResponse, ImportReport, Page } from './lib/types';

  let page: Page = 'dashboard';
  let language: Language = 'en';
  let data: DashboardResponse | null = null;
  let loading = true;
  let error = '';
  let notice = '';
  $: labels = translations[language];

  onMount(() => {
    const saved = localStorage.getItem('runwayclock-language');
    if (saved === 'en' || saved === 'tr') language = saved;
    void refresh();
  });

  async function refresh() {
    error = '';
    try {
      data = await getDashboard();
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      loading = false;
    }
  }

  function navigate(next: Page) {
    page = next;
    notice = '';
  }

  function changeLanguage(event: Event) {
    language = (event.currentTarget as HTMLSelectElement).value as Language;
    localStorage.setItem('runwayclock-language', language);
  }

  async function imported(report: ImportReport) {
    if (report.exact_reimport) {
      notice = labels.exactReimport;
    } else {
      notice = `${labels.importSuccess} ${report.inserted} ${labels.newTransactions}, ${report.duplicates} ${labels.knownRows}, ${report.errors} ${labels.rowErrors}.`;
    }
    if (report.row_errors.length > 0) {
      const details = report.row_errors
        .slice(0, 3)
        .map((row) => `row ${row.row_number}: ${row.message}`)
        .join(' · ');
      notice += ` ${details}`;
    }
    await refresh();
    page = 'dashboard';
  }
</script>

<div class="app-shell">
  <aside class="sidebar">
    <button class="brand" onclick={() => navigate('dashboard')} aria-label="RunwayClock home">
      <span class="brand-mark"><i></i></span>
      <span><strong>Runway</strong><em>Clock</em></span>
    </button>

    <nav aria-label="Main navigation">
      <button class:active={page === 'dashboard'} onclick={() => navigate('dashboard')}>
        <span class="nav-icon">⌁</span>{labels.dashboard}
      </button>
      <button class:active={page === 'import'} onclick={() => navigate('import')}>
        <span class="nav-icon">↥</span>{labels.import}
      </button>
      <button class:active={page === 'review'} onclick={() => navigate('review')}>
        <span class="nav-icon">◇</span>{labels.review}
        {#if data?.summary.unresolved_outflow_count}<b>{data.summary.unresolved_outflow_count}</b>{/if}
      </button>
      <button class:active={page === 'facts'} onclick={() => navigate('facts')}>
        <span class="nav-icon">＋</span>{labels.facts}
      </button>
    </nav>

    <div class="sidebar-bottom">
      <div class="privacy-badge">
        <span>●</span>
        <div><strong>{labels.localOnly}</strong><small>{labels.noCloud}</small></div>
      </div>
      <select class="language-select" value={language} onchange={changeLanguage} aria-label="Language">
        <option value="en">English</option>
        <option value="tr">Türkçe</option>
      </select>
    </div>
  </aside>

  <main>
    <header class="topbar">
      <div class="data-state">
        {#if data?.summary.last_actual_data}
          <i></i><span>{data.summary.transaction_count} actual transactions</span>
        {:else}
          <i class="empty"></i><span>No statement imported</span>
        {/if}
      </div>
      <button class="refresh-button" onclick={refresh} title={labels.refresh}>↻</button>
    </header>

    <div class="content">
      {#if notice}<div class="alert success dismissible"><span>✓ {notice}</span><button onclick={() => (notice = '')}>×</button></div>{/if}
      {#if error}<div class="alert error">{error}</div>{/if}

      {#if loading}
        <div class="loading-state"><span class="spinner"></span><p>{labels.loading}</p></div>
      {:else if page === 'import'}
        <ImportView {labels} onImported={imported} />
      {:else if data}
        {#if page === 'dashboard'}
          <DashboardView {data} {labels} {language} onNavigate={navigate} />
        {:else if page === 'review'}
          <ReviewView
            candidates={data.reviewCandidates}
            unresolvedCount={data.summary.unresolved_outflow_count}
            {labels}
            {language}
            onChanged={refresh}
          />
        {:else if page === 'facts'}
          <FactsView
            scenario={data.scenario}
            rules={data.forecastRules}
            {labels}
            {language}
            onChanged={refresh}
          />
        {/if}
      {/if}
    </div>
  </main>
</div>
