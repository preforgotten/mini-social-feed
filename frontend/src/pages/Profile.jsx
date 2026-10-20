import { createSignal, createResource, Show, For, Switch, Match } from 'solid-js';
import { A, useParams, useNavigate } from '@solidjs/router';
import { session, updateSession } from '../session';
import { api, esc, timeAgo } from '../api';
import PostCard from '../components/PostCard';

export default function ProfilePage() {
  const params = useParams();
  const navigate = useNavigate();
  const s = () => session();

  // Auth guard
  if (!session()) {
    navigate('/login', { replace: true });
  }

  const userId = () => params.userId;
  const isSelf = () => s()?.userId === userId();
  const [tab, setTab] = createSignal('posts');
  const [refresh, setRefresh] = createSignal(0);

  const [user] = createResource(() => [userId(), refresh()], ([id]) => api(`/users/${id}`));
  const [postCount] = createResource(
    () => [userId(), refresh()],
    ([id]) =>
      api(`/users/${id}/posts?limit=1`)
        .then(d => d.total)
        .catch(() => 0),
  );

  const [tabData, { mutate, refetch: refetchTab }] = createResource(
    () => [userId(), tab(), refresh()],
    async ([id, t]) => {
      if (t === 'posts') {
        return api(`/users/${id}/posts?limit=30`).then(r => r.items || []);
      }
      if (t === 'followers') {
        return api(`/users/${id}/followers`).catch(() => []);
      }
      if (t === 'following') {
        return api(`/users/${id}/following`).catch(() => []);
      }
      if (t === 'connections' && isSelf()) {
        return api('/relationships/connections').catch(() => []);
      }
      return [];
    },
  );

  async function toggleFollow() {
    try {
      if (subbed()) {
        await api(`/relationships/${userId()}/subscribe`, { method: 'DELETE' });
      } else {
        await api(`/relationships/${userId()}/subscribe`, { method: 'POST' });
      }
      setRefresh(r => r + 1);
    } catch (e) {
      alert(e.message);
    }
  }

  async function connectReq() {
    try {
      await api(`/relationships/${userId()}/connect`, { method: 'POST' });
      alert('Connection request sent.');
    } catch (e) {
      alert(e.message);
    }
  }

  async function uploadAvatar(e) {
    const file = e.target.files?.[0];
    if (!file) return;
    const form = new FormData();
    form.append('file', file);
    const token = localStorage.getItem('token');
    const res = await fetch('/api/users/me/avatar', {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}` },
      body: form,
    });
    if (!res.ok) {
      alert('Avatar upload failed');
      return;
    }
    const data = await res.json();
    updateSession({ avatarUrl: data.avatar_url });
    setRefresh(r => r + 1);
  }

  // Check follow status from followers/following tab data
  const [subCheck] = createResource(
    () => [userId(), refresh()],
    async ([id]) => {
      const subs = await api('/relationships/subscriptions').catch(() => []);
      setFollowed(subs.some(u => u.id === id));
    },
  );
  const [followed, setFollowed] = createSignal(false);

  // If viewing own profile, always force followed = false (not meaningful)
  const subbed = () => (isSelf() ? false : followed());

  const tabs = () => {
    const list = [
      { key: 'posts', label: 'Posts' },
      { key: 'followers', label: 'Followers' },
      { key: 'following', label: 'Following' },
    ];
    if (isSelf()) {
      list.push({ key: 'connections', label: 'Connections' });
    }
    return list;
  };

  const u = () => user();

  return (
    <>
      <div class="page-header">
        <A href="/" class="back-btn">←</A>
        <span>Profile</span>
      </div>

      <Show when={u()} fallback={<div class="loading-state">Loading profile…</div>}>
        <div class="profile-page">
          <div class="profile-banner" />
          <div class="profile-info">
            <div class="profile-avatar">
              <Show when={u().avatar_url} fallback={u().username[0].toUpperCase()}>
                <img src={u().avatar_url} alt="" />
              </Show>
              <Show when={isSelf()}>
                <label class="avatar-upload-btn">
                  +
                  <input type="file" accept="image/*" onChange={uploadAvatar} hidden />
                </label>
              </Show>
            </div>

            <Show when={!isSelf()}>
              <div class="profile-actions">
                <button
                  class={`btn ${subbed() ? 'following' : 'primary'}`}
                  onClick={toggleFollow}
                >
                  {subbed() ? 'Following' : 'Follow'}
                </button>
                <button class="btn" onClick={connectReq}>
                  Connect
                </button>
              </div>
            </Show>

            <div class="profile-name">{esc(u().username)}</div>
            <div class="profile-handle">@{esc(u().username)}</div>
            <Show when={u().bio}>
              <div class="profile-bio">{esc(u().bio)}</div>
            </Show>
            <div class="profile-meta">
              <span>Joined {new Date(u().created_at * 1000).toLocaleDateString('en-US', { month: 'long', year: 'numeric' })}</span>
              <Show when={u().nostr_pubkey}>
                <span>🔑 Nostr</span>
              </Show>
            </div>
            <div class="profile-stats">
              <a onClick={() => setTab('posts')}>
                <strong>{postCount() || 0}</strong> posts
              </a>
            </div>
          </div>
        </div>

        <div class="tabs">
          <For each={tabs()}>{t => (
            <div
              class="tab"
              classList={{ active: tab() === t.key }}
              onClick={() => setTab(t.key)}
            >
              {t.label}
            </div>
          )}</For>
        </div>

        <Show when={!tabData.loading} fallback={<div class="loading-state">Loading…</div>}>
          <Show when={tabData()?.length} fallback={
            <div class="empty-state"><h3>Nothing here yet</h3></div>
          }>
            <For each={tabData()}>{item =>
              <Switch>
                <Match when={item.content !== undefined}>
                  {/* Post item */}
                  <PostCard post={item} onUpdate={() => setRefresh(r => r + 1)} clickable />
                </Match>
                <Match when={item.username !== undefined}>
                  {/* User list item */}
                  <UserListCard user={item} />
                </Match>
              </Switch>
            }</For>
          </Show>
        </Show>
      </Show>
    </>
  );
}

function UserListCard(props) {
  const u = props.user;
  const initial = () => (u.username || '?')[0].toUpperCase();

  return (
    <A href={`/profile/${u.id}`} class="user-list-card">
      <div class="ul-avatar">
        <Show when={u.avatar_url} fallback={initial()}>
          <img src={u.avatar_url} alt="" />
        </Show>
      </div>
      <div class="ul-info">
        <div class="ul-name">{esc(u.username)}</div>
        <div class="ul-handle">@{esc(u.username)}</div>
        <Show when={u.bio}>
          <div class="ul-bio">{esc(u.bio)}</div>
        </Show>
      </div>
    </A>
  );
}
