const API = '/api';

export async function api(path, opts = {}) {
  const h = { 'Content-Type': 'application/json' };
  const t = localStorage.getItem('token');
  if (t) h['Authorization'] = `Bearer ${t}`;
  const res = await fetch(API + path, { ...opts, headers: { ...h, ...(opts.headers || {}) } });
  if (!res.ok) {
    const b = await res.text(); let m = `${res.status}`;
    try { m = JSON.parse(b).error || m; } catch {}
    throw new Error(m);
  }
  if (res.status === 204) return null;
  return res.json();
}

export function timeAgo(ts) {
  const d = Math.floor(Date.now() / 1000) - ts;
  if (d < 60) return 'now'; if (d < 3600) return `${Math.floor(d / 60)}m`;
  if (d < 86400) return `${Math.floor(d / 3600)}h`;
  if (d < 604800) return `${Math.floor(d / 86400)}d`;
  return new Date(ts * 1000).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

export function esc(s) {
  const d = document.createElement('div'); d.textContent = s || ''; return d.innerHTML;
}
