import './styles.css';

const token = new URLSearchParams(location.search).get('token');
    let state = null;
    let selectedFile = 0;
    let selectedHunk = 0;
    let focus = 'files';
    let screen = 'home';
    let paletteCursor = 0;
    let settings = { has_github_token: false };
    let explainSessions = { available: false, selected_session_id: null, sessions: [] };
    let explainModels = { available: false, selected_model: null, models: [] };
    let explainHistory = { runs: [] };
    let eventSource = null;
    let activeExplainRunId = null;

    const iconFor = (file) => {
      if (file.is_binary) return '◈';
      switch (file.status) {
        case 'Added': return '+';
        case 'Deleted': return '−';
        case 'Renamed': return '→';
        case 'Copied': return '⧉';
        case 'ModeChanged': return '⚙';
        default: return file.hunks.length ? '✎' : '○';
      }
    };
    const markerFor = (status) => status === 'Accepted' ? '[✓]' : status === 'Rejected' ? '[x]' : '[ ]';
    const prefixFor = (kind) => kind === 'Add' ? '+' : kind === 'Remove' ? '-' : ' ';
    const lineClass = (kind) => kind === 'Add' ? 'line-add' : kind === 'Remove' ? 'line-remove' : 'line-context';

    function commandItems() {
      const file = currentFile();
      const reviewAvailable = !!state?.files.length;
      const inReview = screen === 'review' && reviewAvailable;
      const hasHunks = inReview && !!file?.hunks.length;
      return [
        { label: 'Refresh changes', detail: 'Reload the current worktree diff', shortcut: 'r', enabled: true, run: () => mutate('/api/refresh', 'Refreshed review queue.') },
        { label: 'Enter review', detail: 'Open the review workspace', shortcut: 'Enter', enabled: screen === 'home' && reviewAvailable, run: () => enterReview() },
        { label: 'Back to home', detail: 'Return to the better-review home screen', shortcut: 'Esc', enabled: screen === 'review', run: () => { screen = 'home'; focus = 'files'; renderState(state); setStatus('Back on the better-review home screen.'); } },
        { label: 'Focus files', detail: 'Move focus to the changed-file sidebar', shortcut: 'Esc', enabled: inReview && focus === 'hunks', run: () => { focus = 'files'; renderState(state); setStatus('Focused changed files.'); } },
        { label: 'Focus hunks', detail: 'Move focus into the diff hunks', shortcut: 'Enter', enabled: hasHunks, run: () => { focus = 'hunks'; renderState(state); setStatus('Focused diff hunks.'); } },
        { label: 'Accept selection', detail: 'Stage the current file or hunk for commit', shortcut: 'y', enabled: inReview, run: acceptCurrent },
        { label: 'Reject selection', detail: 'Leave the current file or hunk out of the commit', shortcut: 'x', enabled: inReview, run: rejectCurrent },
        { label: 'Move file to unreviewed', detail: 'Unstage the current file and mark it pending', shortcut: 'u', enabled: inReview, run: unreviewCurrent },
        { label: 'Open Explain menu', detail: 'Preview the current file or hunk explanation target', shortcut: 'e', enabled: inReview, run: openExplainMenu },
        { label: 'Choose Explain context', detail: 'Select the opencode session used for Explain', shortcut: 'o', enabled: true, run: openSessionPicker },
        { label: 'Choose Explain model', detail: 'Select the model used for Explain', shortcut: 'm', enabled: true, run: openModelPicker },
        { label: 'Open Explain history', detail: 'Show explanations from this browser session', shortcut: 'h', enabled: true, run: openExplainHistory },
        { label: 'Commit accepted changes', detail: 'Write a commit message for accepted changes', shortcut: 'c', enabled: reviewAvailable, run: () => document.getElementById('commitDialog').showModal() },
        { label: 'Publish current branch', detail: 'Push the reviewed commit from the current branch', shortcut: 'p', enabled: true, run: () => document.getElementById('publishDialog').showModal() },
        { label: 'Open settings', detail: 'Configure GitHub token for HTTPS publishing', shortcut: 's', enabled: true, run: openSettings },
      ];
    }

    function filteredCommandItems() {
      const query = document.getElementById('paletteInput').value.trim().toLowerCase();
      const items = commandItems();
      if (!query) return items;
      return items.filter((item) => `${item.label} ${item.detail} ${item.shortcut}`.toLowerCase().includes(query));
    }

    function openCommandPalette() {
      paletteCursor = 0;
      document.getElementById('paletteInput').value = '';
      renderCommandPalette();
      document.getElementById('commandPalette').showModal();
      document.getElementById('paletteInput').focus();
      setStatus('Command palette opened.');
    }

    function renderCommandPalette() {
      const list = document.getElementById('paletteList');
      const items = filteredCommandItems();
      paletteCursor = clamp(paletteCursor, 0, Math.max(0, items.length - 1));
      list.innerHTML = '';
      if (!items.length) { list.innerHTML = '<li class="palette-item disabled">No commands found</li>'; return; }
      items.forEach((item, index) => {
        const row = document.createElement('li');
        row.className = `palette-item ${index === paletteCursor ? 'selected' : ''} ${item.enabled ? '' : 'disabled'}`;
        row.innerHTML = `<div><strong></strong><span class="palette-detail"></span></div><span class="key"></span>`;
        row.querySelector('strong').textContent = item.label;
        row.querySelector('.palette-detail').textContent = item.detail;
        row.querySelector('.key').textContent = item.shortcut;
        row.addEventListener('mouseenter', () => { paletteCursor = index; renderCommandPalette(); });
        row.addEventListener('click', () => runPaletteCommand(item));
        list.appendChild(row);
      });
    }

    async function runPaletteCommand(item = filteredCommandItems()[paletteCursor]) {
      if (!item) return;
      if (!item.enabled) { setStatus(`${item.label} is unavailable right now.`); return; }
      document.getElementById('commandPalette').close();
      await item.run();
    }

    function connectEvents() {
      if (eventSource) eventSource.close();
      eventSource = new EventSource(`/api/events?token=${encodeURIComponent(token || '')}`);
      eventSource.addEventListener('publish_started', (event) => handleServerEvent(event));
      eventSource.addEventListener('publish_finished', (event) => handleServerEvent(event));
      eventSource.addEventListener('publish_failed', (event) => handleServerEvent(event));
      eventSource.addEventListener('explain_started', (event) => handleServerEvent(event));
      eventSource.addEventListener('explain_finished', (event) => handleServerEvent(event, true));
      eventSource.onerror = () => setStatus('Live updates disconnected. Refresh if actions stop updating.');
    }

    function handleServerEvent(event, refreshHistory = false) {
      try {
        const payload = JSON.parse(event.data);
        setStatus(payload.message);
        if (refreshHistory) {
          openExplainHistory(false)
            .then(() => {
              if (payload.run_id === activeExplainRunId) renderExplainRunAnswer(findExplainRun(payload.run_id));
            })
            .catch(showError);
        }
      } catch (_) {
        setStatus(event.data || 'Received live update.');
      }
    }

    async function request(path, options = {}) {
      const separator = path.includes('?') ? '&' : '?';
      const response = await fetch(`${path}${separator}token=${encodeURIComponent(token || '')}`, {
        headers: { 'content-type': 'application/json', ...(options.headers || {}) },
        ...options,
      });
      if (!response.ok) {
        let message = await response.text();
        try { message = JSON.parse(message).error || message; } catch (_) {}
        throw new Error(message);
      }
      return response.json();
    }

    async function loadState(message = 'Review state loaded.') {
      settings = await request('/api/settings');
      explainSessions = await request('/api/explain/sessions');
      explainModels = await request('/api/explain/models');
      explainHistory = await request('/api/explain/history');
      renderSettingsStatus();
      renderExplainContext();
      renderExplainModel();
      renderState(await request('/api/state'));
      setStatus(message);
    }

    async function mutate(path, message) {
      const result = await request(path, { method: 'POST' });
      renderState(result.state);
      setStatus(result.message || message);
    }

    function renderSettingsStatus() {
      document.getElementById('githubTokenStatus').textContent = settings.has_github_token ? 'GitHub token is saved.' : 'GitHub token is not set.';
    }

    async function openSettings() {
      settings = await request('/api/settings');
      renderSettingsStatus();
      document.getElementById('githubTokenInput').value = '';
      document.getElementById('settingsDialog').showModal();
      document.getElementById('githubTokenInput').focus();
      setStatus('Settings opened.');
    }

    async function saveGithubToken() {
      settings = await request('/api/settings/github-token', {
        method: 'POST',
        body: JSON.stringify({ token: document.getElementById('githubTokenInput').value }),
      });
      renderSettingsStatus();
      document.getElementById('settingsDialog').close();
      document.getElementById('githubTokenInput').value = '';
      setStatus(settings.has_github_token ? 'GitHub token saved.' : 'GitHub token cleared.');
    }

    async function publishCurrentBranch() {
      const result = await request('/api/push', { method: 'POST' });
      renderState(result.state);
      document.getElementById('publishDialog').close();
      setStatus(result.message);
    }

    function selectedExplainSession() {
      return explainSessions.sessions.find((session) => session.id === explainSessions.selected_session_id);
    }

    function explainContextLabel() {
      if (!explainSessions.available) return 'Explain is unavailable because opencode is not ready.';
      const session = selectedExplainSession();
      if (!session) return 'No context source selected.';
      return `${session.title} (${session.id})`;
    }

    function renderExplainContext() {
      const context = document.getElementById('explainContext');
      if (context) context.textContent = explainContextLabel();
    }

    function explainModelLabel() {
      if (!explainModels.available) return 'Explain is unavailable because opencode is not ready.';
      return explainModels.selected_model || 'Auto';
    }

    function renderExplainModel() {
      const model = document.getElementById('explainModel');
      if (model) model.textContent = explainModelLabel();
    }

    async function openSessionPicker() {
      explainSessions = await request('/api/explain/sessions');
      renderExplainContext();
      renderSessionList();
      document.getElementById('sessionDialog').showModal();
      setStatus('Choose an Explain context source.');
    }

    function renderSessionList() {
      const status = document.getElementById('sessionStatus');
      const list = document.getElementById('sessionList');
      list.innerHTML = '';
      if (!explainSessions.available) {
        status.textContent = 'Explain is unavailable because opencode is not ready.';
        return;
      }
      if (!explainSessions.sessions.length) {
        status.textContent = 'No opencode sessions were found for this repository.';
        return;
      }
      status.textContent = 'Select the opencode session to use as Explain context.';
      explainSessions.sessions.forEach((session) => {
        const row = document.createElement('li');
        row.className = `session-item ${session.id === explainSessions.selected_session_id ? 'selected' : ''}`;
        row.innerHTML = '<strong></strong><span class="muted mono"></span><span class="muted"></span>';
        row.querySelector('strong').textContent = session.title || session.id;
        row.querySelector('.mono').textContent = session.id;
        row.querySelectorAll('.muted')[1].textContent = session.directory;
        row.addEventListener('click', () => selectExplainSession(session.id).catch(showError));
        list.appendChild(row);
      });
    }

    async function selectExplainSession(sessionId) {
      explainSessions = await request('/api/explain/session', {
        method: 'POST',
        body: JSON.stringify({ session_id: sessionId }),
      });
      renderExplainContext();
      renderSessionList();
      document.getElementById('sessionDialog').close();
      setStatus(`Explain will use context source ${explainContextLabel()}.`);
    }

    async function openModelPicker() {
      explainModels = await request('/api/explain/models');
      renderExplainModel();
      renderModelList();
      document.getElementById('modelDialog').showModal();
      setStatus('Choose an Explain model.');
    }

    function renderModelList() {
      const status = document.getElementById('modelStatus');
      const list = document.getElementById('modelList');
      list.innerHTML = '';
      if (!explainModels.available) {
        status.textContent = 'Explain is unavailable because opencode is not ready.';
        return;
      }
      status.textContent = 'Choose Auto or a specific opencode model.';
      renderModelRow(list, null, 'Auto');
      explainModels.models.forEach((model) => renderModelRow(list, model, model));
    }

    function renderModelRow(list, model, label) {
      const row = document.createElement('li');
      row.className = `session-item ${model === explainModels.selected_model ? 'selected' : ''}`;
      row.innerHTML = '<strong></strong><span class="muted"></span>';
      row.querySelector('strong').textContent = label;
      row.querySelector('.muted').textContent = model ? 'Explicit model' : 'Use saved/session default when available';
      row.addEventListener('click', () => selectExplainModel(model).catch(showError));
      list.appendChild(row);
    }

    async function selectExplainModel(model) {
      explainModels = await request('/api/explain/model', {
        method: 'POST',
        body: JSON.stringify({ model }),
      });
      renderExplainModel();
      renderModelList();
      document.getElementById('modelDialog').close();
      setStatus(`Explain model set to ${explainModelLabel()}.`);
    }

    async function openExplainHistory(showDialog = true) {
      explainHistory = await request('/api/explain/history');
      renderExplainHistory();
      if (activeExplainRunId !== null) renderExplainRunAnswer(findExplainRun(activeExplainRunId));
      if (showDialog) {
        document.getElementById('historyDialog').showModal();
        setStatus('Explain history opened.');
      }
    }

    function renderExplainHistory() {
      const status = document.getElementById('historyStatus');
      const list = document.getElementById('historyList');
      list.innerHTML = '';
      if (!explainHistory.runs.length) {
        status.textContent = 'No explanations in this session yet.';
        return;
      }
      status.textContent = 'Explain runs from this browser session. Select a run to show its answer.';
      explainHistory.runs.forEach((run) => {
        const row = document.createElement('li');
        row.className = 'history-item';
        row.innerHTML = '<strong></strong><span class="muted"></span><span class="muted"></span>';
        row.querySelector('strong').textContent = run.label;
        row.querySelectorAll('.muted')[0].textContent = `${run.status} · ${run.model}`;
        row.querySelectorAll('.muted')[1].textContent = historyPreview(run);
        row.addEventListener('click', () => showExplainRun(run));
        list.appendChild(row);
      });
    }

    function findExplainRun(id) {
      return explainHistory.runs.find((run) => run.id === id);
    }

    function historyPreview(run) {
      if (run.answer?.summary) return run.answer.summary;
      if (run.error) return run.error;
      return `run ${run.id}`;
    }

    function showExplainRun(run) {
      if (!run) return;
      activeExplainRunId = run.id;
      document.getElementById('explainScope').textContent = run.label;
      renderExplainRunAnswer(run);
      document.getElementById('historyDialog').close();
      const dialog = document.getElementById('explainDialog');
      if (!dialog.open) dialog.showModal();
      setStatus(`Showing Explain run ${run.id}.`);
    }

    function renderExplainRunAnswer(run) {
      const answer = document.getElementById('explainAnswer');
      answer.innerHTML = '';
      answer.classList.toggle('muted', !run?.answer);
      if (!run) {
        answer.textContent = 'No explanation has been requested yet.';
        return;
      }
      if (run.status === 'Running') {
        answer.textContent = `Running Explain for ${run.label} with ${run.model}.`;
        return;
      }
      if (run.status === 'Failed') {
        answer.textContent = run.error || 'Explain failed.';
        return;
      }
      if (!run.answer) {
        answer.textContent = `${run.status} · no answer payload was returned.`;
        return;
      }
      renderExplainSection(answer, 'Summary', run.answer.summary);
      renderExplainSection(answer, 'Purpose', run.answer.purpose);
      renderExplainSection(answer, 'Change', run.answer.change);
      renderExplainSection(answer, `Risk (${run.answer.risk_level})`, run.answer.risk_reason);
    }

    function renderExplainSection(parent, title, text) {
      const section = document.createElement('section');
      section.className = 'explain-section';
      const heading = document.createElement('strong');
      heading.textContent = title;
      const body = document.createElement('p');
      body.textContent = text || 'Not provided.';
      section.append(heading, body);
      parent.appendChild(section);
    }

    function explainTargetLabel() {
      const file = currentFile();
      if (!file) return 'No selection';
      if (focus === 'hunks' && file.hunks.length) return `hunk ${file.display_label} ${file.hunks[selectedHunk].header}`;
      return `file ${file.display_label}`;
    }

    async function openExplainMenu() {
      explainSessions = await request('/api/explain/sessions');
      explainModels = await request('/api/explain/models');
      document.getElementById('explainScope').textContent = explainTargetLabel();
      renderExplainContext();
      renderExplainModel();
      activeExplainRunId = null;
      renderExplainRunAnswer(null);
      document.getElementById('explainDialog').showModal();
      setStatus('Explain menu opened.');
    }

    async function requestExplainPreview() {
      const file = currentFile();
      if (!file) return;
      const payload = {
        file_index: selectedFile,
        hunk_index: focus === 'hunks' && file.hunks.length ? selectedHunk : null,
      };
      const run = await request('/api/explain', {
        method: 'POST',
        body: JSON.stringify(payload),
      });
      activeExplainRunId = run.id;
      document.getElementById('explainScope').textContent = run.label;
      explainHistory = await request('/api/explain/history');
      renderExplainHistory();
      renderExplainRunAnswer(findExplainRun(run.id) || run);
      setStatus(`Explain started for ${run.label}.`);
    }

    function renderState(nextState) {
      state = nextState;
      selectedFile = clamp(selectedFile, 0, Math.max(0, state.files.length - 1));
      const file = currentFile();
      selectedHunk = clamp(selectedHunk, 0, Math.max(0, (file?.hunks.length || 1) - 1));

      document.getElementById('repo').textContent = state.repo_path;
      document.getElementById('pending').textContent = state.counts.unreviewed;
      document.getElementById('accepted').textContent = state.counts.accepted;
      document.getElementById('rejected').textContent = state.counts.rejected;
      if (!state.files.length) screen = 'home';
      renderHome();
      renderFiles();
      renderDiff();
      renderFooter();
      renderLayout();
    }

    function renderLayout() {
      const onHome = screen === 'home';
      document.getElementById('home').classList.toggle('hidden', !onHome);
      document.getElementById('workspace').classList.toggle('hidden', onHome);
      document.getElementById('footer').classList.toggle('hidden', onHome);
    }

    function renderHome() {
      const total = state.counts.unreviewed + state.counts.accepted + state.counts.rejected;
      const reviewed = state.counts.accepted + state.counts.rejected;
      const progress = total ? Math.round((reviewed / total) * 100) : 0;
      let title = 'No changes';
      let detail = 'Run your coding agent or make changes, then refresh the review queue.';
      if (total && state.counts.unreviewed) {
        title = 'Ready to review';
        detail = 'Open the review workspace and accept only the file or hunk changes that belong.';
      } else if (state.counts.accepted) {
        title = 'Ready to commit';
        detail = 'All current review items have a decision. Commit accepted staged changes when ready.';
      } else if (total) {
        title = 'Nothing accepted';
        detail = 'Rejected changes stay in your worktree and are left out of the commit.';
      }
      document.getElementById('homeTitle').innerHTML = `${title.replace('review', '<span>review</span>')}`;
      document.getElementById('homeDetail').textContent = detail;
      document.getElementById('homeProgress').style.width = `${progress}%`;
      document.getElementById('homeCounts').textContent = `${state.counts.unreviewed} pending · ${state.counts.accepted} accepted · ${state.counts.rejected} rejected`;
      document.getElementById('enterReview').disabled = !state.files.length;
    }

    function enterReview() {
      if (!state?.files.length) { setStatus('No reviewable changes yet. Refresh after making changes.'); return; }
      screen = 'review';
      focus = 'files';
      renderState(state);
      setStatus('Review workspace ready.');
    }

    function renderFiles() {
      const files = document.getElementById('files');
      files.innerHTML = '';
      if (!state.files.length) {
        files.innerHTML = '<li class="empty">No reviewable changes.<br><span class="muted">Run your agent, then refresh.</span></li>';
        return;
      }
      state.files.forEach((file, index) => {
        const item = document.createElement('li');
        item.className = `file ${index === selectedFile ? 'selected' : ''}`;
        const stats = lineStats(file);
        item.innerHTML = `
          <span class="selection-bar">${index === selectedFile ? '▌' : ' '}</span>
          <span class="review-marker ${file.review_status.toLowerCase()}">${markerFor(file.review_status)}</span>
          <span class="file-label"><span class="file-icon">${iconFor(file)}</span> <span class="mono"></span></span>
          <span class="stats">+${stats.added} -${stats.removed}</span>`;
        item.querySelector('.mono').textContent = file.display_label;
        item.addEventListener('click', () => { selectedFile = index; selectedHunk = 0; focus = 'files'; screen = 'review'; renderState(state); });
        files.appendChild(item);
      });
    }

    function renderDiff() {
      const diff = document.getElementById('diff');
      const title = document.getElementById('diffTitle');
      const file = currentFile();
      diff.innerHTML = '';
      if (!file) {
        title.textContent = 'Review';
        diff.innerHTML = '<div class="empty">No changes to review.</div>';
        return;
      }

      title.textContent = file.display_label;
      if (file.is_binary || !file.hunks.length) {
        diff.innerHTML = `<div class="binary-card"><h2>${file.is_binary ? 'Binary file' : 'No text hunks'}</h2><p>${file.is_binary ? 'This change cannot be shown as a text diff.' : 'This file changed, but there is no patch body to render.'}</p></div>`;
        return;
      }

      file.hunks.forEach((hunk, hunkIndex) => {
        const section = document.createElement('section');
        section.className = `hunk ${focus === 'hunks' && hunkIndex === selectedHunk ? 'selected' : ''}`;
        section.innerHTML = `
          <div class="hunk-header">
            <code></code>
            <div class="hunk-actions">
              <span class="review-marker ${hunk.review_status.toLowerCase()}">${markerFor(hunk.review_status)}</span>
              <button data-action="accept-hunk">Accept</button>
              <button data-action="reject-hunk" class="danger">Reject</button>
            </div>
          </div>
          <table class="diff-table"><tbody></tbody></table>`;
        section.querySelector('code').textContent = hunk.header;
        section.querySelector('[data-action="accept-hunk"]').addEventListener('click', () => mutate(`/api/files/${selectedFile}/hunks/${hunkIndex}/accept`, 'Accepted hunk.').catch(showError));
        section.querySelector('[data-action="reject-hunk"]').addEventListener('click', () => mutate(`/api/files/${selectedFile}/hunks/${hunkIndex}/reject`, 'Rejected hunk.').catch(showError));
        const body = section.querySelector('tbody');
        hunk.lines.forEach((line) => body.appendChild(renderDiffLine(line)));
        diff.appendChild(section);
      });
      scrollSelectedHunkIntoView();
    }

    function renderDiffLine(line) {
      const row = document.createElement('tr');
      row.className = lineClass(line.kind);
      row.innerHTML = `
        <td class="line-no">${line.old_line ?? ''}</td>
        <td class="line-no">${line.new_line ?? ''}</td>
        <td class="line-prefix">${prefixFor(line.kind)}</td>
        <td class="line-content"></td>`;
      row.querySelector('.line-content').textContent = line.content;
      return row;
    }

    function renderFooter() {
      const file = currentFile();
      document.getElementById('position').textContent = `${state.files.length ? selectedFile + 1 : 0} / ${state.files.length}`;
      document.getElementById('footerPath').textContent = file ? file.display_label : 'No selection';
      document.getElementById('focusLabel').textContent = file && focus === 'hunks' ? `hunk ${selectedHunk + 1}/${Math.max(file.hunks.length, 1)}` : 'file';
      const stats = file ? lineStats(file) : { added: 0, removed: 0 };
      document.getElementById('lineStats').textContent = `+${stats.added} -${stats.removed}`;
    }

    function lineStats(file) {
      return file.hunks.reduce((stats, hunk) => {
        hunk.lines.forEach((line) => {
          if (line.kind === 'Add') stats.added += 1;
          if (line.kind === 'Remove') stats.removed += 1;
        });
        return stats;
      }, { added: 0, removed: 0 });
    }

    function currentFile() { return state?.files[selectedFile]; }
    function clamp(value, min, max) { return Math.min(max, Math.max(min, value)); }
    function setStatus(message) { document.getElementById('status').textContent = message; document.getElementById('homeStatus').textContent = message; }
    function showError(error) { setStatus(error.message); }
    function scrollSelectedHunkIntoView() {
      if (focus !== 'hunks') return;
      document.querySelector('.hunk.selected')?.scrollIntoView({ block: 'nearest' });
    }

    async function acceptCurrent() {
      const file = currentFile();
      if (!file) return;
      if (focus === 'hunks' && file.hunks.length) {
        await mutate(`/api/files/${selectedFile}/hunks/${selectedHunk}/accept`, 'Accepted hunk.');
      } else {
        await mutate(`/api/files/${selectedFile}/accept`, 'Accepted file.');
      }
    }
    async function rejectCurrent() {
      const file = currentFile();
      if (!file) return;
      if (focus === 'hunks' && file.hunks.length) {
        await mutate(`/api/files/${selectedFile}/hunks/${selectedHunk}/reject`, 'Rejected hunk.');
      } else {
        await mutate(`/api/files/${selectedFile}/reject`, 'Rejected file.');
      }
    }
    async function unreviewCurrent() {
      if (!currentFile()) return;
      await mutate(`/api/files/${selectedFile}/unreview`, 'Moved file back to unreviewed.');
    }

    document.getElementById('refresh').addEventListener('click', () => mutate('/api/refresh', 'Refreshed review queue.').catch(showError));
    document.getElementById('homeRefresh').addEventListener('click', () => mutate('/api/refresh', 'Refreshed review queue.').catch(showError));
    document.getElementById('enterReview').addEventListener('click', enterReview);
    document.getElementById('homeCommit').addEventListener('click', () => document.getElementById('commitDialog').showModal());
    document.getElementById('openSettings').addEventListener('click', () => openSettings().catch(showError));
    document.getElementById('acceptCurrent').addEventListener('click', () => acceptCurrent().catch(showError));
    document.getElementById('rejectCurrent').addEventListener('click', () => rejectCurrent().catch(showError));
    document.getElementById('unreviewCurrent').addEventListener('click', () => unreviewCurrent().catch(showError));
    document.getElementById('openExplain').addEventListener('click', () => openExplainMenu().catch(showError));
    document.getElementById('chooseExplainContext').addEventListener('click', (event) => { event.preventDefault(); openSessionPicker().catch(showError); });
    document.getElementById('chooseExplainModel').addEventListener('click', (event) => { event.preventDefault(); openModelPicker().catch(showError); });
    document.getElementById('openExplainHistory').addEventListener('click', (event) => { event.preventDefault(); openExplainHistory().catch(showError); });
    document.getElementById('requestExplain').addEventListener('click', (event) => { event.preventDefault(); requestExplainPreview(); });
    document.getElementById('openCommit').addEventListener('click', () => document.getElementById('commitDialog').showModal());
    document.getElementById('publishCurrent').addEventListener('click', () => document.getElementById('publishDialog').showModal());
    document.getElementById('submitPublish').addEventListener('click', (event) => { event.preventDefault(); publishCurrentBranch().catch(showError); });
    document.getElementById('saveGithubToken').addEventListener('click', (event) => { event.preventDefault(); saveGithubToken().catch(showError); });
    document.getElementById('paletteInput').addEventListener('input', () => { paletteCursor = 0; renderCommandPalette(); });
    document.getElementById('paletteInput').addEventListener('keydown', (event) => {
      if (event.key === 'Escape') { document.getElementById('commandPalette').close(); setStatus('Command palette closed.'); event.preventDefault(); }
      else if (event.key === 'ArrowDown' || event.key === 'j') { paletteCursor += 1; renderCommandPalette(); event.preventDefault(); }
      else if (event.key === 'ArrowUp' || event.key === 'k') { paletteCursor -= 1; renderCommandPalette(); event.preventDefault(); }
      else if (event.key === 'Enter') { runPaletteCommand().catch(showError); event.preventDefault(); }
    });

    document.getElementById('submitCommit').addEventListener('click', async (event) => {
      event.preventDefault();
      try {
        const message = document.getElementById('commitMessage').value;
        const result = await request('/api/commit', { method: 'POST', body: JSON.stringify({ message }) });
        document.getElementById('commitDialog').close();
        document.getElementById('commitMessage').value = '';
        renderState(result.state);
        setStatus(result.message);
        document.getElementById('publishDialog').showModal();
      } catch (error) { showError(error); }
    });

    document.addEventListener('keydown', (event) => {
      if ((event.ctrlKey || event.metaKey) && (event.key === 'p' || event.key === 'k')) {
        event.preventDefault();
        openCommandPalette();
        return;
      }
      if (event.target.closest('textarea, dialog')) return;
      const file = currentFile();
      if (screen === 'home') {
        if (event.key === 'Enter') { enterReview(); event.preventDefault(); }
        else if (event.key === 'r') { mutate('/api/refresh', 'Refreshed review queue.').catch(showError); event.preventDefault(); }
        else if (event.key === 'c') { document.getElementById('commitDialog').showModal(); event.preventDefault(); }
        else if (event.key === 'p') { document.getElementById('publishDialog').showModal(); event.preventDefault(); }
        else if (event.key === 's') { openSettings().catch(showError); event.preventDefault(); }
        else if (event.key === 'o') { openSessionPicker().catch(showError); event.preventDefault(); }
        else if (event.key === 'm') { openModelPicker().catch(showError); event.preventDefault(); }
        else if (event.key === 'h') { openExplainHistory().catch(showError); event.preventDefault(); }
        return;
      }
      if (event.key === 'j' || event.key === 'ArrowDown') {
        if (focus === 'hunks' && file?.hunks.length) selectedHunk = clamp(selectedHunk + 1, 0, file.hunks.length - 1);
        else { selectedFile = clamp(selectedFile + 1, 0, Math.max(0, (state?.files.length || 1) - 1)); selectedHunk = 0; }
        renderState(state); event.preventDefault();
      } else if (event.key === 'k' || event.key === 'ArrowUp') {
        if (focus === 'hunks' && file?.hunks.length) selectedHunk = clamp(selectedHunk - 1, 0, file.hunks.length - 1);
        else { selectedFile = clamp(selectedFile - 1, 0, Math.max(0, (state?.files.length || 1) - 1)); selectedHunk = 0; }
        renderState(state); event.preventDefault();
      } else if (event.key === 'Enter') {
        if (file?.hunks.length) focus = 'hunks'; renderState(state); event.preventDefault();
      } else if (event.key === 'Escape') {
        if (focus === 'hunks') focus = 'files'; else screen = 'home'; renderState(state); event.preventDefault();
      } else if (event.key === 'Tab') {
        if (file?.hunks.length) { selectedHunk = (selectedHunk + 1) % file.hunks.length; focus = 'hunks'; renderState(state); }
        event.preventDefault();
      } else if (event.key === 'y') acceptCurrent().catch(showError);
      else if (event.key === 'x') rejectCurrent().catch(showError);
      else if (event.key === 'u') unreviewCurrent().catch(showError);
      else if (event.key === 'e') openExplainMenu().catch(showError);
      else if (event.key === 'o') openSessionPicker().catch(showError);
      else if (event.key === 'm') openModelPicker().catch(showError);
      else if (event.key === 'h') openExplainHistory().catch(showError);
      else if (event.key === 'r') mutate('/api/refresh', 'Refreshed review queue.').catch(showError);
      else if (event.key === 'c') document.getElementById('commitDialog').showModal();
      else if (event.key === 'p') document.getElementById('publishDialog').showModal();
      else if (event.key === 's') openSettings().catch(showError);
    });

    connectEvents();
    loadState().catch(showError);
