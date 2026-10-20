import { createSignal, Show, For } from 'solid-js';
import { A, useNavigate } from '@solidjs/router';
import { api, timeAgo, esc } from '../api';

/**
 * Renders a post with optional inline reply toggle.
 * Props:
 *   post       - post object
 *   onUpdate   - callback after reply/mutation (optional)
 *   clickable  - if true, clicking the card navigates to /thread/:id
 *   depth      - for thread nesting (adds left border when > 0)
 */
export default function PostCard(props) {
  const navigate = useNavigate();

  const p = props.post;
  const initial = () => (p.author_username || '?')[0].toUpperCase();

  function handleCardClick(e) {
    if (!props.clickable) return;
    // Don't navigate if user clicked a link or button
    if (e.target.closest('a, button')) return;
    navigate(`/thread/${p.id}`);
  }

  return (
    <div class="post-card" classList={{
      clickable: !!props.clickable,
      'thread-reply': (props.depth || 0) > 0,
    }} style={props.depth ? `margin-left:${props.depth * 22}px` : ''}
      onClick={handleCardClick}>
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

        <Show when={!props.clickable} fallback={
          <div class="post-content">{esc(p.content)}</div>
        }>
          <A href={`/thread/${p.id}`} class="post-content-link">
            <div class="post-content">{esc(p.content)}</div>
          </A>
        </Show>

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
          <InlineReplyBtn post={p} onUpdate={props.onUpdate} />
          <A href={`/thread/${p.id}`} class="action-link">↳ Thread</A>
        </div>
      </div>
    </div>
  );
}

function InlineReplyBtn(props) {
  const [showForm, setShowForm] = createSignal(false);
  const [reply, setReply] = createSignal('');
  const [sending, setSending] = createSignal(false);

  async function sendReply() {
    const text = reply().trim();
    if (!text) return;
    setSending(true);
    try {
      await api('/posts', {
        method: 'POST',
        body: JSON.stringify({ content: text, parent_id: props.post.id }),
      });
      setReply('');
      setShowForm(false);
      props.onUpdate?.();
    } catch (e) {
      alert(e.message);
    }
    setSending(false);
  }

  return (
    <>
      <button onClick={() => setShowForm(!showForm())}>↩ Reply</button>
      <Show when={showForm()}>
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
    </>
  );
}
