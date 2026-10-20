import { createResource, Show, For } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { session } from '../session';
import { api } from '../api';

export default function MediaPage() {
  const navigate = useNavigate();
  const s = () => session();

  // Auth guard
  if (!session()) {
    navigate('/login', { replace: true });
  }

  const [data] = createResource(() =>
    api(`/users/${s().userId}/posts?limit=50`),
  );

  const allMedia = () => {
    const items = [];
    const posts = data()?.items || [];
    for (const p of posts) {
      for (const m of p.media || []) {
        items.push({ ...m, postId: p.id });
      }
    }
    return items;
  };

  return (
    <>
      <div class="page-header">
        <A href="/" class="back-btn">←</A>
        <span>My Media</span>
      </div>

      <Show when={!data.loading} fallback={<div class="loading-state">Loading media…</div>}>
        <Show when={!data.error} fallback={
          <div class="error-state">{data.error?.message || 'Failed to load media'}</div>
        }>
          <Show when={allMedia().length > 0} fallback={
            <div class="empty-state">
              <h3>No uploads yet</h3>
              <p>Images you attach to posts will appear here.</p>
            </div>
          }>
            <div class="media-grid">
              <For each={allMedia()}>{m => (
                <div class="media-item" classList={{ deleted: m.deleted }}>
                  <Show when={!m.deleted} fallback={
                    <div class="media-tombstone">[removed]</div>
                  }>
                    <A href={`/thread/${m.postId}`}>
                      <img src={m.url} alt="" loading="lazy" />
                    </A>
                  </Show>
                </div>
              )}</For>
            </div>
          </Show>
        </Show>
      </Show>
    </>
  );
}
