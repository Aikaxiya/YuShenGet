// IPC 封装层：所有跨进程调用集中在这里。
// 不在 Tauri 里运行时（纯浏览器预览）自动切到 demo 数据，方便单独调 UI。
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  AccountRow,
  AccountStats,
  CharacterProfile,
  LocalLink,
  SyncOutcome,
  SyncProgress,
} from './types';

export const isDesktop =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

/**
 * 演示数据必须惰性加载：静态 import 会让它在模块求值时执行，
 * 一旦那段生成逻辑有问题，整个前端会在挂载前卡死（白屏且无报错）。
 */
async function loadDemo() {
  return import('./mock');
}

export async function scanLocalLinks(): Promise<LocalLink[]> {
  if (!isDesktop) {
    return [];
  }
  return invoke<LocalLink[]>('scan_local_links');
}

export async function syncRecords(url: string | null, auto: boolean): Promise<SyncOutcome> {
  if (!isDesktop) {
    const { demoStats } = await loadDemo();
    await wait(700);
    return {
      uid: demoStats.uid,
      fetched: demoStats.total_pulls,
      added: 0,
      total: demoStats.total_pulls,
      stopped_early: false,
    };
  }
  return invoke<SyncOutcome>('sync_records', { url, auto });
}

/** 批量取角色档案（元素 + 头像），一次 IPC 搞定整个列表。 */
export async function loadCharacterProfiles(
  names: string[],
  download = false,
): Promise<CharacterProfile[]> {
  if (!isDesktop) {
    return names.map((name) => ({ name, element: 'unknown', avatar: null }));
  }
  return invoke<CharacterProfile[]>('character_profiles', { names, download });
}

export async function listAccounts(): Promise<AccountRow[]> {
  if (!isDesktop) {
    const { demoAccounts } = await loadDemo();
    return demoAccounts;
  }
  return invoke<AccountRow[]>('list_accounts');
}

export async function getStats(uid: string | null): Promise<AccountStats> {
  if (!isDesktop) {
    const { demoStats } = await loadDemo();
    await wait(120);
    return demoStats;
  }
  return invoke<AccountStats>('get_stats', { uid });
}

export async function deleteAccount(uid: string): Promise<void> {
  if (!isDesktop) {
    return;
  }
  await invoke<void>('delete_account', { uid });
}

export async function renameAccount(uid: string, nickname: string): Promise<void> {
  if (!isDesktop) {
    return;
  }
  await invoke<void>('rename_account', { uid, nickname });
}

export async function exportUigf(uid: string): Promise<string> {
  if (!isDesktop) {
    return JSON.stringify({ info: { uid, uigf_version: 'v4.0' }, hk4e: [] }, null, 2);
  }
  return invoke<string>('export_uigf', { uid });
}

export async function onSyncProgress(
  handler: (progress: SyncProgress) => void,
): Promise<UnlistenFn | null> {
  if (!isDesktop) {
    return null;
  }
  return listen<SyncProgress>('sync-progress', (event) => {
    handler(event.payload);
  });
}

/**
 * 读取系统剪贴板。桌面端走后端（不触发 WebView2 权限弹窗），
 * 浏览器预览环境才退回 navigator.clipboard。
 */
export async function readClipboardText(): Promise<string> {
  if (isDesktop) {
    return invoke<string>('read_clipboard_text');
  }
  return navigator.clipboard.readText();
}

/** UID 打码：只保留前段与末位，避免截图或共享时泄漏完整账号。 */
export function maskUid(uid: string | null | undefined): string {
  if (!uid) {
    return '未知账号';
  }
  if (uid.length <= 4) {
    return '****';
  }
  return `${uid.slice(0, uid.length - 3)}***`;
}

export function formatDate(value: string | null | undefined): string {
  if (!value) {
    return '未知';
  }
  return value.replace('T', ' ').slice(0, 10);
}

export function formatDateTime(value: string | null | undefined): string {
  if (!value) {
    return '未知';
  }
  return value.replace('T', ' ').slice(0, 16);
}
