import { createSignal, Show, For } from 'solid-js';
import { session } from '../session';
import { api } from '../api';

/**
 * Post creation composer.
 * Props:
 *   onPost - called after successful post creation (optional)
 */
export default function Composer(props) {
  const [content, setContent] = createSignal('');
  const [posting, setPosting] = createSignal(false);
  const [uploading, setUploading] = createSignal(false);
  const [mediaIds, setMediaIds] = createSignal([]);

  const s = () => session() || {};
  const initial = () => (s().username || '?')[0].toUpperCase();
  const remaining = () => 16384 - content().length;
  const canPost = () => content().trim().length > 0 && !posting();

  async function handlePost() {
    if (!content().trim()) return;
    setPosting(true);
    try {
      const body = { content: content().trim() };
      if (mediaIds().length) body.media_ids = mediaIds();
      await api('/posts', { method: 'POST', body: JSON.stringify(body) });
      setContent('');
      setMediaIds([]);
      props.onPost?.();
    } catch (e) {
      alert(e.message);
    }
    setPosting(false);
  }

  async function handleUpload(e) {
    const file = e.target.files?.[0];
    if (!file) return;
    setUploading(true);
    try {
      const form = new FormData();
      form.append('file', file);
      const token = localStorage.getItem('token');
      const res = await fetch('/api/media', {
        method: 'POST',
        headers: { Authorization: `Bearer ${token}` },
        body: form,
      });
      if (!res.ok) throw new Error('Upload failed');
      const m = await res.json();
      setMediaIds(prev => [...prev, m.id]);
      e.target.value = '';
    } catch (err) {
      alert(err.message);
    }
    setUploading(false);
  }

  function removeMedia(id) {
    setMediaIds(prev => prev.filter(mid => mid !== id));
  }

  function onKeyDown(e) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      handlePost();
    }
  }

  return (
    <div class="composer">
      <div class="composer-avatar">
        <Show when={s().avatarUrl} fallback={initial()}>
          <img src={s().avatarUrl} alt="" />
        </Show>
      </div>
      <div class="composer-body">
        <textarea
          placeholder="What's on your mind?"
          maxLength={16384}
          value={content()}
          onInput={e => setContent(e.target.value)}
          onKeyDown={onKeyDown}
          rows={2}
        />
        <Show when={mediaIds().length > 0}>
          <div class="media-preview">
            <For each={mediaIds()}>{id => (
              <div class="media-preview-item">
                <img src={`/api/media/${id}`} alt="" />
                <button class="remove-media" onClick={() => removeMedia(id)}>✕</button>
              </div>
            )}</For>
          </div>
        </Show>
        <div class="composer-footer">
          <div class="composer-left">
            <label class="upload-btn">
              {uploading() ? '[uploading]' : '+ Image'}
              <input type="file" accept="image/*" onChange={handleUpload} disabled={uploading()} hidden />
            </label>
          </div>
          <div style="display:flex;align-items:center;gap:12px">
            <span class="char-count" classList={{ warn: remaining() < 100 }}>
              {remaining().toLocaleString()}
            </span>
            <button class="post-btn" onClick={handlePost} disabled={!canPost()}>
              {posting() ? '...' : 'Post'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
