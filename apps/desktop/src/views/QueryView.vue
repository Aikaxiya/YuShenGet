<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import type { UnlistenFn } from '@tauri-apps/api/event';

import {
  deleteAccount,
  formatDate,
  getStats,
  listAccounts,
  maskUid,
  onSyncProgress,
  readClipboardText,
  renameAccount,
  scanLocalLinks,
  syncRecords,
} from '../api';
import type { AccountRow } from '../types';

const router = useRouter();

const url = ref('');
const busy = ref(false);
const error = ref('');
const progress = ref('');
const accounts = ref<AccountRow[]>([]);
const localLinkCount = ref(0);
const helpOpen = ref(false);
const editing = ref(false);
const pendingDelete = ref<string | null>(null);
const renamingUid = ref<string | null>(null);
const renameValue = ref('');
const urlInput = ref<HTMLTextAreaElement | null>(null);

let unlisten: UnlistenFn | null = null;

const canSubmit = computed(() => !busy.value);

/** 从剪贴板读链接。桌面端走后端直接读系统剪贴板，不触发权限弹窗。 */
async function pasteFromClipboard() {
  error.value = '';
  progress.value = '';
  try {
    const text = await readClipboardText();
    if (!text.trim()) {
      error.value = '剪贴板里没有内容';
      return;
    }
    url.value = text.trim();
  } catch {
    urlInput.value?.focus();
    error.value = '读取剪贴板失败，请点输入框后按 Ctrl+V 粘贴';
  }
}

function clearUrl() {
  url.value = '';
  error.value = '';
  progress.value = '';
  urlInput.value?.focus();
}

async function refresh() {
  // 历史记录是主数据，失败才值得报错；扫描本地缓存是增强项，
  // 没装游戏/目录不存在都属正常，不能连累主列表。
  try {
    accounts.value = await listAccounts();
  } catch (err) {
    error.value = String(err);
    return;
  }

  try {
    const links = await scanLocalLinks();
    localLinkCount.value = links.length;
  } catch {
    localLinkCount.value = 0;
  }
}

onMounted(async () => {
  unlisten = await onSyncProgress((payload) => {
    if (payload.phase === 'fetching') {
      progress.value = `正在抓取${payload.pool}祈愿…已取回 ${payload.fetched} 条`;
    }
  });
  await refresh();
});

onUnmounted(() => {
  if (unlisten) {
    unlisten();
  }
});

async function submit(auto: boolean) {
  if (busy.value) {
    return;
  }
  if (!auto && url.value.trim().length === 0) {
    error.value = '请先粘贴祈愿分析链接';
    return;
  }
  busy.value = true;
  error.value = '';
  progress.value = auto ? '正在从本地缓存读取链接…' : '正在连接祈愿记录接口…';

  try {
    const outcome = await syncRecords(auto ? null : url.value.trim(), auto);
    progress.value = `完成：新增 ${outcome.added} 条，累计 ${outcome.total} 条`;
    await refresh();
    router.push({ name: 'analysis', params: { uid: outcome.uid } });
  } catch (err) {
    // 失败时清掉进度文案，否则会一直挂着「正在读取…」误导用户
    progress.value = '';
    error.value = String(err);
  } finally {
    busy.value = false;
  }
}

async function openAccount(account: AccountRow) {
  const stats = await getStats(account.uid);
  if (stats.total_pulls === 0) {
    error.value = '该账号还没有记录，请重新同步';
    return;
  }
  router.push({ name: 'analysis', params: { uid: account.uid } });
}

function startRename(account: AccountRow) {
  renamingUid.value = account.uid;
  renameValue.value = account.nickname ?? '';
}

async function commitRename() {
  if (renamingUid.value === null) {
    return;
  }
  await renameAccount(renamingUid.value, renameValue.value.trim());
  renamingUid.value = null;
  await refresh();
}

async function confirmDelete(uid: string) {
  await deleteAccount(uid);
  pendingDelete.value = null;
  await refresh();
}
</script>

<template>
  <div class="topbar">
    <h1 class="topbar__title">提瓦特祈愿分析</h1>
    <button class="icon-btn" title="全部数据仅保存在本机" @click="helpOpen = !helpOpen">?</button>
  </div>

  <h2 class="hero">祈愿抽卡分析</h2>

  <div class="panel input-panel">
    <textarea
      ref="urlInput"
      v-model="url"
      class="url-input"
      spellcheck="false"
      placeholder="请粘贴祈愿分析地址，例如：https://public-operation-hk4e.mihoyo.com/gacha_info/api/getGachaLog?authkey=...&game_biz=hk4e_cn&gacha_type=301&page=1&size=20&end_id=0"
    />
    <div class="input-toolbar">
      <span class="char-count faint">{{ url.trim().length }} 字符</span>
      <button class="tool-btn" :disabled="busy" @click="pasteFromClipboard">粘贴</button>
      <button class="tool-btn" :disabled="busy || url.length === 0" @click="clearUrl">清空</button>
    </div>
  </div>

  <button class="primary-btn query-btn" :disabled="!canSubmit" @click="submit(false)">
    {{ busy ? '查询中…' : '点击查询' }}
  </button>

  <button class="ghost-btn auto-btn" :disabled="busy" @click="submit(true)">
    从本地缓存读取{{ localLinkCount > 0 ? `（找到 ${localLinkCount} 条）` : '（需先在游戏内打开祈愿记录）' }}
  </button>

  <p v-if="progress" class="progress">{{ progress }}</p>
  <p v-if="error" class="error-text">{{ error }}</p>

  <button class="help-toggle" @click="helpOpen = !helpOpen">
    <span>如何获取抽卡分析地址？</span>
    <span class="chevron" :class="{ 'chevron--open': helpOpen }">›</span>
  </button>

  <div v-if="helpOpen" class="panel help-panel">
    <ol>
      <li>
        <strong>手机上安装 HoYoGet（APK）</strong>，按它的提示导出自己的祈愿记录链接。
      </li>
      <li>把导出的链接发到电脑：微信 / QQ 传给自己，或存到网盘再下载。</li>
      <li>在电脑上复制这条链接，回到本页点「<strong>粘贴</strong>」，再点「点击查询」。</li>
      <li>
        也可以不走手机：先在游戏内打开一次「祈愿 → 祈愿记录」，再点「从本地缓存读取」，
        程序会直接从本机缓存里找链接{{ localLinkCount > 0 ? `（当前找到 ${localLinkCount} 条）` : '' }}。
      </li>
      <li>兜底方式：手动复制 <code>getGachaLog</code> 开头的完整链接，粘贴到上方输入框。</li>
    </ol>

    <p class="faint">
      链接里的 authkey 等同临时凭据，有效期约 24 小时，过期需重新导出。
      链接只在内存中转发给官方接口，不写入日志、不上传任何服务器。
    </p>
  </div>

  <div class="section-title">历史记录</div>

  <div class="history-actions">
    <button class="link-btn" @click="editing = !editing">
      {{ editing ? '完成编辑' : '修改 / 删除记录' }}
    </button>
  </div>

  <div v-if="accounts.length === 0" class="panel empty-panel">
    还没有本地记录。完成一次查询后会自动出现在这里。
  </div>

  <ul class="account-list">
    <li v-for="account in accounts" :key="account.uid" class="account-item panel">
      <button class="account-main" @click="editing ? null : openAccount(account)">
        <span class="account-name">
          {{ account.nickname ? `${account.nickname}（${maskUid(account.uid)}）` : maskUid(account.uid) }}
        </span>
        <span class="account-meta">
          {{ account.pull_count }} 抽 · {{ formatDate(account.last_time) }}
        </span>
      </button>

      <div v-if="editing" class="account-edit">
        <template v-if="renamingUid === account.uid">
          <input v-model="renameValue" class="rename-input" maxlength="20" placeholder="备注名" />
          <button class="mini-btn" @click="commitRename">保存</button>
          <button class="mini-btn ghost" @click="renamingUid = null">取消</button>
        </template>
        <template v-else>
          <button class="mini-btn ghost" @click="startRename(account)">重命名</button>
          <button
            v-if="pendingDelete !== account.uid"
            class="mini-btn danger"
            @click="pendingDelete = account.uid"
          >
            删除
          </button>
          <button v-else class="mini-btn danger" @click="confirmDelete(account.uid)">
            确认删除
          </button>
        </template>
      </div>
    </li>
  </ul>

  <p class="footnote faint">
    所有祈愿记录只保存在本机，不上传任何服务器。<br />
    原石折算按 160 原石 / 抽。
  </p>
</template>

<style scoped>
.hero {
  margin: 18px 0 22px;
  text-align: center;
  font-size: 32px;
  line-height: 1.2;
  font-weight: 700;
  letter-spacing: 3px;
  background: linear-gradient(180deg, #fff6d6 8%, var(--gold) 52%, #b98f3d 100%);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
  filter: drop-shadow(0 3px 10px rgba(180, 140, 60, 0.28));
}

.input-panel {
  padding: 12px;
}

.url-input {
  width: 100%;
  height: 132px;
  padding: 10px 12px;
  border: none;
  border-radius: var(--radius-md);
  outline: none;
  resize: none;
  font-size: 12.5px;
  line-height: 1.7;
  background: rgba(18, 13, 32, 0.6);
  user-select: text;
}

.url-input::placeholder {
  color: var(--text-faint);
}

.input-toolbar {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
}

.char-count {
  flex: 1;
  font-size: 11px;
  padding-left: 2px;
}

.tool-btn {
  padding: 4px 12px;
  border-radius: 7px;
  font-size: 12px;
  color: var(--gold-light);
  background: rgba(232, 197, 106, 0.12);
  border: 1px solid rgba(232, 197, 106, 0.28);
  transition: background 0.16s ease;
}

.tool-btn:hover:not(:disabled) {
  background: rgba(232, 197, 106, 0.22);
}

.tool-btn:disabled {
  opacity: 0.42;
  cursor: not-allowed;
}

.query-btn {
  margin-top: 14px;
}

.auto-btn {
  margin-top: 10px;
  font-size: 13px;
}

.progress {
  margin: 10px 0 0;
  font-size: 12.5px;
  color: var(--gold-light);
}

.help-toggle {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 14px;
  padding: 12px 14px;
  border-radius: var(--radius-md);
  font-size: 13.5px;
  color: var(--text);
  background: rgba(58, 46, 90, 0.5);
  border: 1px solid var(--border);
}

.help-panel {
  margin-top: 8px;
  padding: 12px 16px;
  font-size: 13px;
  color: var(--text-dim);
}

.help-panel ol {
  margin: 0;
  padding-left: 18px;
}

.help-panel li {
  margin: 6px 0;
}

.help-panel code {
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 12px;
  color: var(--gold-light);
  background: rgba(0, 0, 0, 0.28);
}

.chevron {
  display: inline-block;
  font-size: 18px;
  color: var(--text-faint);
  transition: transform 0.18s ease;
}

.chevron--open {
  transform: rotate(90deg);
}

.history-actions {
  display: flex;
  justify-content: flex-end;
  margin: -4px 0 8px;
}

.link-btn {
  font-size: 12.5px;
  color: var(--gold);
  padding: 2px 4px;
}

.link-btn:hover {
  color: var(--gold-light);
}

.empty-panel {
  padding: 18px;
  text-align: center;
  font-size: 13px;
  color: var(--text-faint);
}

.account-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.account-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 12px 14px;
}

.account-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  text-align: left;
}

.account-name {
  font-size: 14px;
  color: var(--text);
}

.account-meta {
  font-size: 12px;
  color: var(--text-faint);
}

.account-edit {
  display: flex;
  align-items: center;
  gap: 6px;
}

.mini-btn {
  padding: 5px 10px;
  border-radius: 8px;
  font-size: 12px;
  color: #2a1f08;
  background: linear-gradient(180deg, #f0d68c, #c9a24c);
}

.mini-btn.ghost {
  color: var(--text-dim);
  background: rgba(255, 255, 255, 0.08);
}

.mini-btn.danger {
  color: #ffd5dd;
  background: rgba(200, 70, 96, 0.5);
}

.rename-input {
  width: 96px;
  padding: 5px 8px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: rgba(18, 13, 32, 0.7);
  font-size: 12px;
  outline: none;
  user-select: text;
}

.footnote {
  margin-top: 26px;
  font-size: 11.5px;
  line-height: 1.8;
  text-align: center;
}

</style>
