// 統一發票對獎台 — 前端邏輯（無框架、無 CDN，完全離線可跑）
(() => {
  'use strict';

  const $ = (id) => document.getElementById(id);
  const el = {
    health: $('health'),
    version: $('version'),
    dataset: $('dataset'),
    dropzone: $('dropzone'),
    file: $('file'),
    preview: $('preview'),
    form: $('manual'),
    code: $('code'),
    period: $('period'),
    result: $('result'),
    body: $('result-body'),
    periods: $('periods'),
    btnUpdate: $('btn-update'),
    btnCloud: $('btn-cloud'),
    msg: $('update-msg'),
  };

  const fmtMoney = (n) =>
    'NT$' + Number(n).toLocaleString('zh-TW');

  // ---------- 資料狀態 ----------
  async function loadStatus() {
    try {
      const r = await fetch('/api/status', { cache: 'no-store' });
      const j = await r.json();
      el.health.className = 'dot ok';
      el.version.textContent = 'v' + (j.app_version || '—');

      const periods = j.periods || [];
      const latest = periods[0];
      el.dataset.textContent = latest
        ? `資料：${latest.label}　·　${periods.length} 期　·　來源 ${j.dataset_origin === 'embedded' ? '內建' : '已更新'}`
        : '資料：無';

      el.period.innerHTML = '';
      for (const p of periods) {
        const o = document.createElement('option');
        o.value = p.period;
        o.textContent = p.label;
        el.period.appendChild(o);
      }

      el.periods.innerHTML = periods.map(renderPeriodCard).join('');
    } catch (e) {
      el.health.className = 'dot bad';
      el.dataset.textContent = '無法連上本機服務';
    }
  }

  function renderPeriodCard(p) {
    const nums = [p.special, p.grand, ...(p.first || [])]
      .map((n, i) => {
        const tag = i === 0 ? '特別獎' : i === 1 ? '特獎' : '頭獎';
        return `<span><span class="tag">${tag}</span> ${escapeHtml(n)}</span>`;
      })
      .join('');
    return `<div class="pcard">
      <div class="phead"><span>${escapeHtml(p.label)}</span>
        <span class="range">領獎 ${escapeHtml(p.redeem_start)} ~ ${escapeHtml(p.redeem_end)}</span></div>
      <div class="nums">${nums}</div>
    </div>`;
  }

  // ---------- 對獎結果 ----------
  function renderResult(j, imageUrl) {
    el.result.hidden = false;
    el.result.className = 'panel result ' + (j.is_winner ? 'win' : 'lose');

    const head = j.is_winner
      ? `<div class="prize-banner"><span>${escapeHtml(j.best_prize)}</span>
           <span class="amt">${fmtMoney(j.best_amount)}</span></div>`
      : `<div class="prize-banner miss">未中獎</div>`;

    const cloudLabel = {
      hit: '已比對，中獎',
      miss: '已比對，未中獎',
      not_loaded: '未下載本期名單',
    }[j.cloud_state] || j.cloud_state;

    const rows = [
      ['發票號碼', j.code],
      ['期別', `${j.period}（${j.period_code}）`],
      ['開獎日', j.draw_date],
      ['領獎期間', `${j.redeem_start} ~ ${j.redeem_end}　${deadlineText(j.days_left)}`],
      ['雲端專屬獎', cloudLabel],
    ];
    if (j.issue_date) rows.splice(2, 0, ['開立日期', j.issue_date]);
    if (typeof j.total_amount === 'number') rows.splice(3, 0, ['總計額', fmtMoney(j.total_amount)]);
    if (j.winner_tax_id) rows.push(['買受人統編', '已打統編（營業人不得領獎）']);

    const kv = `<dl class="kv">${rows
      .map(([k, v]) => `<dt>${escapeHtml(k)}</dt><dd>${escapeHtml(String(v))}</dd>`)
      .join('')}</dl>`;

    const hits = (j.hits || []).length
      ? `<ul class="hits">${j.hits
          .map(
            (h) =>
              `<li><b>${escapeHtml(h.prize)}</b>　${fmtMoney(h.amount)}　—　${escapeHtml(h.detail)}</li>`
          )
          .join('')}</ul>`
      : '';

    const notes = (j.notes || []).length
      ? `<ul class="notes">${j.notes.map((n) => `<li>${escapeHtml(n)}</li>`).join('')}</ul>`
      : '';

    const pic = imageUrl
      ? `<div class="preview"><img src="${imageUrl}" alt="上傳的發票照片"></div>`
      : '';

    el.body.innerHTML = head + kv + hits + notes + pic;
    el.result.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  function deadlineText(days) {
    if (typeof days !== 'number') return '';
    if (days < 0) return `（已逾期 ${Math.abs(days)} 天）`;
    if (days === 0) return '（今天最後一天）';
    return `（剩 ${days} 天）`;
  }

  function renderError(msg, extra) {
    el.result.hidden = false;
    el.result.className = 'panel result lose';
    el.body.innerHTML =
      `<div class="prize-banner miss">無法完成比對</div>
       <p class="hint">${escapeHtml(msg)}</p>` +
      (extra ? `<ul class="notes"><li>${escapeHtml(extra)}</li></ul>` : '');
  }

  // ---------- 上傳照片 ----------
  async function scanFile(file) {
    if (!file) return;
    if (!file.type.startsWith('image/')) {
      renderError('請選擇圖片檔（JPEG / PNG / WebP）');
      return;
    }
    const url = URL.createObjectURL(file);
    el.preview.innerHTML = `<img src="${url}" alt="預覽">`;
    el.msg.textContent = '正在辨識 QR…';

    try {
      const r = await fetch('/api/scan', {
        method: 'POST',
        headers: { 'Content-Type': file.type || 'application/octet-stream' },
        body: file,
      });
      const j = await r.json();
      el.msg.textContent = '';
      if (j.ok) {
        renderResult(j, url);
      } else {
        renderError(j.error || '辨識失敗', (j.payloads || []).join(' / '));
      }
    } catch (e) {
      el.msg.textContent = '';
      renderError('上傳失敗：' + e.message);
    }
  }

  async function scanBlob(blob) {
    const f = new File([blob], 'paste.png', { type: blob.type || 'image/png' });
    await scanFile(f);
  }

  // ---------- 手動輸入 ----------
  el.form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const code = el.code.value.replace(/\s|-/g, '');
    if (code.length < 8) {
      renderError('請輸入 8 碼數字或 10 碼字軌號碼');
      return;
    }
    el.msg.textContent = '查詢中…';
    try {
      const q = new URLSearchParams({ code, period: el.period.value || '' });
      const r = await fetch('/api/check?' + q.toString(), { cache: 'no-store' });
      const j = await r.json();
      el.msg.textContent = '';
      if (j.ok) renderResult(j, null);
      else renderError(j.error || '查詢失敗');
    } catch (e) {
      el.msg.textContent = '';
      renderError('查詢失敗：' + e.message);
    }
  });

  // ---------- 拖放 / 貼上 ----------
  el.dropzone.addEventListener('click', () => el.file.click());
  el.dropzone.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      el.file.click();
    }
  });
  el.file.addEventListener('change', () => scanFile(el.file.files[0]));

  ['dragenter', 'dragover'].forEach((ev) =>
    el.dropzone.addEventListener(ev, (e) => {
      e.preventDefault();
      el.dropzone.classList.add('over');
    })
  );
  ['dragleave', 'drop'].forEach((ev) =>
    el.dropzone.addEventListener(ev, (e) => {
      e.preventDefault();
      el.dropzone.classList.remove('over');
    })
  );
  el.dropzone.addEventListener('drop', (e) => {
    const f = e.dataTransfer?.files?.[0];
    if (f) scanFile(f);
  });
  window.addEventListener('paste', (e) => {
    for (const item of e.clipboardData?.items || []) {
      if (item.type.startsWith('image/')) {
        const blob = item.getAsFile();
        if (blob) scanBlob(blob);
        break;
      }
    }
  });

  // ---------- 更新 ----------
  el.btnUpdate.addEventListener('click', async () => {
    el.btnUpdate.disabled = true;
    el.msg.textContent = '正在向官方站台取得最新中獎號碼…';
    try {
      const r = await fetch('/api/update', { method: 'POST' });
      const j = await r.json();
      el.msg.textContent = j.ok
        ? `已更新至 ${j.latest}（${(j.bytes / 1024).toFixed(1)} KB，SHA-256 已驗證）`
        : '更新失敗：' + j.error;
      if (j.ok) loadStatus();
    } catch (e) {
      el.msg.textContent = '更新失敗：' + e.message;
    } finally {
      el.btnUpdate.disabled = false;
    }
  });

  el.btnCloud.addEventListener('click', async () => {
    el.btnCloud.disabled = true;
    el.msg.textContent = '正在下載雲端發票專屬獎名單（可能數十 MB，請稍候）…';
    try {
      const r = await fetch('/api/cloud', { method: 'POST' });
      const j = await r.json();
      el.msg.textContent = j.ok
        ? `已載入：${j.detail}（${(j.bytes / 1048576).toFixed(1)} MB）`
        : '下載失敗：' + j.error;
    } catch (e) {
      el.msg.textContent = '下載失敗：' + e.message;
    } finally {
      el.btnCloud.disabled = false;
    }
  });

  function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({
      '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
    }[c]));
  }

  loadStatus();
})();
