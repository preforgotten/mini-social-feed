import { createResource, Show, For } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { session } from '../session';
import { api } from '../api';
import Composer from '../components/Composer';
import PostCard from '../components/PostCard';

export default function FeedPage() {
  const navigate = useNavigate();

  // Auth guard
  if (!session()) {
    navigate('/login', { replace: true });
  }

  const [posts, { refetch }] = createResource(
    () => api('/posts?limit=30').then(d => d.items),
  );

  return (
    <>
      <div class="page-header">Feed</div>

      <Composer onPost={refetch} />

      <Show when={!posts.loading} fallback={<div class="loading-state">Loading feed…</div>}>
        <Show when={!posts.error} fallback={
          <div class="error-state">{posts.error?.message || 'Failed to load feed'}</div>
        }>
          <Show when={posts()?.length} fallback={
            <div class="empty-state">
              <h3>Welcome!</h3>
              <p>No posts yet. Be the first to share something.</p>
            </div>
          }>
            <For each={posts()}>{p =>
              <PostCard post={p} onUpdate={refetch} clickable />
            }</For>
          </Show>
        </Show>
      </Show>
    </>
  );
}
