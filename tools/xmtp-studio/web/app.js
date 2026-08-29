/**
 * LibXMTP Studio Client Logic
 */

document.addEventListener('DOMContentLoaded', () => {
  initTabs();
  loadConfig();
  initListeners();
});

function initTabs() {
  const tabs = document.querySelectorAll('.nav-tab');
  tabs.forEach(tab => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.nav-tab').forEach(t => t.classList.toggle('active', t === tab));
      document.querySelectorAll('.tab-pane').forEach(p => p.classList.toggle('active', p.id === `tab-${tab.dataset.tab}`));
    });
  });
}

async function loadConfig() {
  try {
    const res = await fetch('/api/config');
    const data = await res.json();

    const select = document.getElementById('select-group');
    select.innerHTML = '';

    data.groups.forEach(g => {
      const opt = document.createElement('option');
      opt.value = g.groupId;
      opt.textContent = `${g.name} (${g.membersCount} Members)`;
      select.appendChild(opt);
    });
  } catch (e) {
    console.error(e);
  }
}

function initListeners() {
  // Send MLS Message
  document.getElementById('msg-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const groupId = document.getElementById('select-group').value;
    const text = document.getElementById('input-text').value;
    const resultBox = document.getElementById('msg-result-box');

    try {
      const res = await fetch('/api/mls/send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ groupId, text }),
      });
      const data = await res.json();

      if (data.success) {
        appendMessageRow(data.message);
        resultBox.innerHTML = `
          <div class="card" style="border-color: #e11d48; background: rgba(225, 29, 72, 0.08);">
            <strong style="color: #fda4af;">🔒 MLS Message Encrypted & Broadcasted!</strong>
            <div class="mono text-muted mt-1" style="font-size: 0.75rem;">Epoch: ${data.message.epoch} • Ciphertext: ${data.message.ciphertext.slice(0, 16)}...</div>
          </div>
        `;
        document.getElementById('input-text').value = '';
      }
    } catch (err) {
      resultBox.innerHTML = `<div class="badge red">Send error: ${err.message}</div>`;
    }
  });

  // Register Identity
  document.getElementById('identity-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const walletAddress = document.getElementById('identity-wallet').value;
    const box = document.getElementById('identity-json-box');

    try {
      const res = await fetch('/api/identity/register', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ walletAddress }),
      });
      const data = await res.json();
      box.textContent = JSON.stringify(data, null, 2);
    } catch (err) {
      box.textContent = `Error: ${err.message}`;
    }
  });
}

function appendMessageRow(msg) {
  const container = document.getElementById('messages-container');
  const empty = container.querySelector('.empty-state');
  if (empty) container.innerHTML = '';

  const row = document.createElement('div');
  row.className = 'ledger-row';
  row.innerHTML = `
    <div>
      <div style="font-weight: 700; color: #fff;">"${msg.plaintextPreview}"</div>
      <div class="mono text-muted" style="font-size: 0.72rem;">Sender: ${msg.sender.slice(0, 14)}...</div>
    </div>
    <div style="text-align: right;">
      <div style="color: #e11d48; font-weight: 700; font-family: var(--font-mono);">Epoch #${msg.epoch}</div>
      <div class="mono text-muted" style="font-size: 0.72rem;">${msg.ratchetState}</div>
    </div>
  `;
  container.insertBefore(row, container.firstChild);
}
