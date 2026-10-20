import { createSignal, createResource, Show, For } from 'solid-js';
import { A, useParams, useNavigate } from '@solidjs/router';
import { session } from '../session';
import { api, esc, timeAgo } from '../api';

export default function ThreadPage() {
  const params = useParams();
  const navigate = useNavigate();

  // Auth guard
  if (!session()) {
    navigate('/login', { replace: true });
  }

  const postId = () => params.postId;
  const [thread, { refetch }] = createResource(postId, id =>
    api(`/posts/${id}/thread`),
  );

  return (
    <>
      <div class="page-header">
        <A href="/" class="back-btn">←</A>
        <span>Thread</span>
      </div>

      <Show when={!thread.loading} fallback={<div class="loading-state">Loading thread…</div>}>
        <Show when={!thread.error} fallback={
          <div class="error-state">{thread.error?.message || 'Not found'}</div>
        }>
          <Show when={thread()} fallback={
            <div class="error-state">Thread not found</div>
          }>
            <ThreadTree node={thread()} depth={0} onUpdate={refetch} />
          </Show>
        </Show>
      </Show>
    </>
  );
}

function ThreadTree(props) {
  const node = props.node;
  const p = node.post;
  const depth = props.depth || 0;
  const [showReply, setShowReply] = createSignal(false);
  const [reply, setReply] = createSignal('');
  const [sending, setSending] = createSignal(false);
  const initial = () => (p.author_username || '?')[0].toUpperCase();

  async function sendReply() {
    const text = reply().trim();
    if (!text) return;
    setSending(true);
    try {
      await api('/posts', {
        method: 'POST',
        body: JSON.stringify({ content: text, parent_id: p.id }),
      });
      setReply('');
      setShowReply(false);
      props.onUpdate();
    } catch (e) {
      alert(e.message);
    }
    setSending(false);
  }

  return (
    <>
      <div class="post-card" classList={{ 'thread-reply': depth > 0 }}
        style={depth ? `margin-left:${depth * 22}px` : ''}>
        <A href={`/profile/${p.author_id}`} class="post-avatar">
          <Show when={p.author_avatar_url} fallback={initial()}>
            <img src={p.author_avatar_url} alt="" />
          </Show>
        </A>
        <div class="post-body">
          <div class="post-header">
            <A href={`/profile/${p.author_id}`} class="post-author">{esc(p.author_username)}</A>
            <span class="post-handle">@{esc(p.author_username)}</span>
            <span class="post-time">· {timeAgo(p.created_at)}</span>
          </div>
          <div class="post-content">{esc(p.content)}</div>

          <Show when={p.media?.length}>
            <div class="post-media">
              <For each={p.media}>{m =>
                <Show when={!m.deleted} fallback={<div class="media-tombstone">[removed]</div>}>
                  <img src={m.url} alt="" loading="lazy" />
                </Show>
              }</For>
            </div>
          </Show>

          <div class="post-actions">
            <button onClick={() => setShowReply(!showReply())}>↩ Reply</button>
          </div>

          <Show when={showReply()}>
            <div class="reply-form">
              <textarea
                placeholder="Write a reply..."
                value={reply()}
                onInput={e => setReply(e.target.value)}
                rows={2}
              />
              <button onClick={sendReply} disabled={sending()}>
                {sending() ? '...' : 'Reply'}
              </button>
            </div>
          </Show>
        </div>
      </div>

      <Show when={node.replies?.length}>
        <For each={node.replies}>{child =>
          <ThreadTree node={child} depth={depth + 1} onUpdate={props.onUpdate} />
        }</For>
      </Show>
    </>
  );
}
