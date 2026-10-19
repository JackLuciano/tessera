/**
 * Dev-only fake backend so the real UI can run in a plain browser (vite dev) for screenshots and
 * layout work. Never bundled into the app: it is imported dynamically behind `import.meta.env.DEV`
 * and only when the Tauri runtime is absent. All names and sizes are made up.
 */
import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import type { Cat, Kind, TreeNode } from '../api';

interface N { id: number; name: string; parent: number; size: number; files: number; dirs: number; kind: Kind; cat: Cat; modified: number; children: number[] }

const GB = 1024 ** 3, MB = 1024 ** 2;
const now = Math.floor(Date.now() / 1000);
const nodes: N[] = [];
let seed = 7;
const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);

type Spec = [name: string, size: number, cat: Cat, kids?: Spec[]];
const many = (prefix: string, n: number, total: number, cat: Cat, ext = ''): Spec[] =>
  Array.from({ length: n }, (_, i) => [`${prefix} ${String(i + 1).padStart(2, '0')}${ext}`, total * (0.5 + rnd()) / n, cat] as Spec);

const TREE: Spec = ['C:\\', 0, 'system', [
  ['Users', 0, 'docs', [['alex', 0, 'docs', [
    ['Videos', 0, 'video', [['Vacation 2025', 0, 'video', many('Clip', 14, 48 * GB, 'video', '.mp4')], ['Recordings', 0, 'video', many('Gameplay', 9, 31 * GB, 'video', '.mkv')]]],
    ['Pictures', 0, 'images', [['Camera Roll', 0, 'images', many('IMG', 30, 22 * GB, 'images', '.heic')], ['Wallpapers', 6 * GB, 'images']]],
    ['Documents', 0, 'docs', [['Projects', 0, 'dev', [
      ['webshop', 0, 'dev', [['node_modules', 2.4 * GB, 'dev'], ['.next', 1.1 * GB, 'cache'], ['src', 0.3 * GB, 'dev']]],
      ['game-engine', 0, 'dev', [['target', 9.5 * GB, 'dev'], ['assets', 3.2 * GB, 'images']]],
      ['ml-notebooks', 0, 'dev', [['__pycache__', 0.4 * GB, 'cache'], ['datasets', 12 * GB, 'archives']]],
    ]], ['Invoices', 1.2 * GB, 'docs'], ['Scans', 2.6 * GB, 'docs']]],
    ['Downloads', 0, 'archives', [...many('installer', 6, 8 * GB, 'apps', '.exe'), ...many('backup', 4, 26 * GB, 'archives', '.zip'), ['ubuntu-24.04.iso', 5.8 * GB, 'archives']]],
    ['AppData', 0, 'cache', [['Local', 0, 'cache', [['Temp', 7.2 * GB, 'cache'], ['Google', 0, 'cache', [['Chrome', 0, 'cache', [['Cache', 3.4 * GB, 'cache']]]]], ['npm-cache', 4.1 * GB, 'cache'], ['Packages', 5.5 * GB, 'apps']]], ['Roaming', 6.3 * GB, 'apps']]],
  ]]]],
  ['Games', 0, 'games', [['Steam', 0, 'games', [['steamapps', 0, 'games', [['Starfield', 128 * GB, 'games'], ['Cyberpunk 2077', 71 * GB, 'games'], ['Counter-Strike 2', 34 * GB, 'games'], ['Hades II', 9 * GB, 'games']]]]], ['Epic Games', 0, 'games', [['Fortnite', 46 * GB, 'games']]]]],
  ['Program Files', 0, 'apps', [['Adobe', 14 * GB, 'apps'], ['Microsoft Office', 4.2 * GB, 'apps'], ['NVIDIA Corporation', 2.8 * GB, 'apps'], ['Blender Foundation', 1.9 * GB, 'apps'], ['Docker', 3.6 * GB, 'apps']]],
  ['Program Files (x86)', 0, 'apps', [['Steam', 1.2 * GB, 'apps'], ['Microsoft', 1.6 * GB, 'apps']]],
  ['Windows', 0, 'system', [['WinSxS', 11 * GB, 'system'], ['System32', 7.5 * GB, 'system'], ['SoftwareDistribution', 0, 'system', [['Download', 3.1 * GB, 'system']]], ['Installer', 4.4 * GB, 'system']]],
  ['ProgramData', 0, 'apps', [['Docker', 18 * GB, 'apps'], ['Package Cache', 2.2 * GB, 'cache']]],
  ['pagefile.sys', 16 * GB, 'system'],
  ['hiberfil.sys', 12.7 * GB, 'system'],
]];

function add(s: Spec, parent: number): number {
  const id = nodes.length;
  const [name, size, cat, kids] = s;
  const isDir = !!kids || !/\.\w{2,4}$/.test(name) && size === 0;
  nodes.push({ id, name, parent, size: 0, files: 0, dirs: 0, kind: kids ? 'dir' : /\.\w{2,4}$/.test(name) ? 'file' : 'dir', cat, modified: now - Math.floor(rnd() * 900) * 86400, children: [] });
  if (kids) {
    for (const k of kids) nodes[id].children.push(add(k, id));
  } else if (!isDir && nodes[id].kind === 'dir') {
    // Leaf folder: give it a few big files plus a small-files bucket so drilling down has content
    const n = 3 + Math.floor(rnd() * 4);
    for (let i = 0; i < n; i++) nodes[id].children.push(add([`part-${i + 1}.bin`, size * 0.7 / n, cat], id));
    const b = nodes.length;
    nodes.push({ id: b, name: `${120 + Math.floor(rnd() * 900)} small files`, parent: id, size: size * 0.3, files: 900, dirs: 0, kind: 'bucket', cat, modified: now - 86400 * 3, children: [] });
    nodes[id].children.push(b);
  } else {
    nodes[id].size = size; nodes[id].files = 1;
  }
  const n = nodes[id];
  if (n.children.length) {
    n.size = n.files = n.dirs = 0;
    for (const c of n.children) { const k = nodes[c]; n.size += k.size; n.files += k.files; n.dirs += k.dirs + (k.kind === 'dir' ? 1 : 0); }
    n.children.sort((a, b) => nodes[b].size - nodes[a].size);
  }
  return id;
}
add(TREE, -1);

const pathOf = (id: number): string => {
  const parts: string[] = [];
  for (let c = id; c > 0; c = nodes[c].parent) parts.unshift(nodes[c].name);
  return 'C:\\' + parts.join('\\');
};
const catsOf = (id: number) => {
  const m = new Map<Cat, number>();
  const walk = (i: number) => { const n = nodes[i]; if (!n.children.length) m.set(n.cat, (m.get(n.cat) ?? 0) + n.size); n.children.forEach(walk); };
  walk(id);
  return [...m].map(([cat, size]) => ({ cat, size })).sort((a, b) => b.size - a.size);
};
const dto = (id: number, depth: number, max: number, min: number, top = true): TreeNode => {
  const n = nodes[id];
  return {
    id, name: n.name, path: pathOf(id), size: n.size, files: n.files, dirs: n.dirs, kind: n.kind, cat: catsOf(id)[0]?.cat ?? n.cat,
    modified: n.modified, child_count: n.children.length,
    children: depth > 0 ? n.children.filter(c => nodes[c].size >= (top ? 1 : min)).slice(0, max).map(c => dto(c, depth - 1, max, min, false)) : [],
  };
};
const summary = () => ({ engine: 'mft', root: 'C:\\', size: nodes[0].size, files: nodes[0].files, dirs: nodes[0].dirs, elapsed_ms: 4210, errors: 37, cats: catsOf(0) });
const files = () => nodes.filter(n => n.kind === 'file').sort((a, b) => b.size - a.size);
const junkRule = (key: string, label: string, hint: string, safe: boolean, names: string[]) => {
  const hits = nodes.filter(n => names.includes(n.name));
  return { key, label, hint, safe, size: hits.reduce((s, n) => s + n.size, 0), ids: hits.map(n => n.id), sample: hits.slice(0, 4).map(n => pathOf(n.id)) };
};

let activeProfile = 'gaming';

async function fakeScan() {
  const total = nodes[0];
  for (let i = 1; i <= 12; i++) {
    await new Promise(r => setTimeout(r, 110));
    await emit('scan-progress', { files: Math.round(total.files * i / 12), dirs: Math.round(total.dirs * i / 12), bytes: Math.round(total.size * i / 12) });
  }
  return summary();
}

export function installMock() {
  mockIPC(async (cmd, args) => {
    const a = (args ?? {}) as Record<string, any>;
    switch (cmd) {
      case 'list_drives': return [
        { name: 'Windows', mount: 'C:\\', fs: 'NTFS', total: 1000 * GB, available: 1000 * GB - nodes[0].size * 1.04, removable: false, kind: 'SSD' },
        { name: 'Data', mount: 'D:\\', fs: 'NTFS', total: 2000 * GB, available: 1240 * GB, removable: false, kind: 'HDD' },
        { name: 'USB', mount: 'E:\\', fs: 'exFAT', total: 128 * GB, available: 91 * GB, removable: true, kind: 'Unknown' },
      ];
      case 'is_elevated': return true;
      case 'scan': return fakeScan();
      case 'load_cached':
        if (new URLSearchParams(location.search).has('fresh')) return null;
        setTimeout(() => emit('catchup-done', { folders: 214, delta: 1.3 * GB, size: nodes[0].size, files: nodes[0].files }), 2500);
        return { ...summary(), engine: 'cache', cached_at: now - 3 * 3600 };
      case 'get_summary': return summary();
      case 'get_node': return dto(a.id, Math.min(a.depth, 6), a.maxChildren, a.minFrac ? nodes[a.id].size * a.minFrac : 0);
      case 'get_ancestors': { const out: [number, string][] = []; for (let c = a.id; c >= 0; c = nodes[c].parent) out.unshift([c, nodes[c].name]); return out; }
      case 'largest_files': return files().slice(0, a.limit).map(n => ({ id: n.id, name: n.name, path: pathOf(n.id), size: n.size, cat: n.cat, modified: n.modified }));
      case 'find_junk': return [
        junkRule('node_modules', 'node_modules folders', 'Reinstall with npm/pnpm install', true, ['node_modules']),
        junkRule('cargo_target', 'Rust target folders', 'Rebuilt by cargo build', true, ['target']),
        junkRule('temp', 'Temp folders', 'Temporary files left behind by apps', true, ['Temp']),
        junkRule('pkg_cache', 'Package manager caches', 'npm, pnpm, pip, NuGet, yarn — re-downloaded on demand', true, ['npm-cache']),
        junkRule('browser', 'Browser caches', 'Chrome / Edge / Firefox cache, logins are not touched', true, ['Cache']),
        junkRule('next', 'Next.js / Nuxt build caches', '.next, .nuxt, .svelte-kit — rebuilt on next build', true, ['.next']),
        junkRule('pycache', 'Python caches', '__pycache__ and .pytest_cache', true, ['__pycache__']),
        junkRule('wu', 'Windows Update downloads', 'Already-installed update packages', false, ['Download']),
      ].filter(g => g.size > 0).sort((x, y) => y.size - x.size);
      case 'find_duplicates': {
        for (let i = 1; i <= 6; i++) { await new Promise(r => setTimeout(r, 120)); await emit('dup-progress', { hashed: i * 4 * GB, total: 24 * GB }); }
        const f = files().filter(n => n.name.endsWith('.zip') || n.name.endsWith('.mp4')).slice(0, 9);
        return [0, 3, 6].map(i => ({ size: f[i].size, wasted: f[i].size * 2, files: f.slice(i, i + 3).map(n => ({ id: n.id, path: pathOf(n.id), modified: n.modified })) }));
      }
      case 'history_list': return Array.from({ length: 9 }, (_, i) => ({ ts: now - (8 - i) * 6 * 86400, size: nodes[0].size * (0.82 + i * 0.022 + rnd() * 0.01), files: Math.round(nodes[0].files * (0.9 + i * 0.012)) }));
      case 'history_diff': return [
        { path: 'C:\\Users\\alex\\Videos', before: 52 * GB, after: 79 * GB, delta: 27 * GB },
        { path: 'C:\\Games\\Steam\\steamapps', before: 196 * GB, after: 242 * GB, delta: 46 * GB },
        { path: 'C:\\Users\\alex\\Downloads', before: 61 * GB, after: 44 * GB, delta: -17 * GB },
        { path: 'C:\\ProgramData\\Docker', before: 12 * GB, after: 18 * GB, delta: 6 * GB },
        { path: 'C:\\Windows\\SoftwareDistribution', before: 5.2 * GB, after: 3.1 * GB, delta: -2.1 * GB },
      ];
      case 'opt_status': return {
        supported: true, elevated: true, active: activeProfile, power_plans: [
          { guid: '381b4222-f694-41f0-9685-ff5bb260df2e', name: 'Balanced', active: activeProfile !== 'gaming' },
          { guid: '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c', name: 'High performance', active: activeProfile === 'gaming' },
          { guid: 'e9a42b02-d5df-448d-aa00-03f14749eb61', name: 'Ultimate Performance', active: false },
        ],
        tweaks: [
          { key: 'game_mode', on: activeProfile !== 'work' && activeProfile !== 'battery', admin: false, reboot: false },
          { key: 'no_game_dvr', on: activeProfile === 'gaming', admin: false, reboot: false },
          { key: 'perf_visuals', on: activeProfile === 'gaming' || activeProfile === 'battery', admin: false, reboot: true },
          { key: 'no_transparency', on: activeProfile === 'battery', admin: false, reboot: false },
          { key: 'no_background_apps', on: activeProfile === 'gaming' || activeProfile === 'battery', admin: false, reboot: false },
          { key: 'raw_mouse', on: activeProfile === 'gaming', admin: false, reboot: false },
          { key: 'focus', on: activeProfile === 'gaming' || activeProfile === 'work', admin: false, reboot: false },
          { key: 'hags', on: activeProfile === 'gaming', admin: true, reboot: true },
          { key: 'net_throttling', on: activeProfile === 'gaming', admin: true, reboot: true },
          { key: 'game_priority', on: activeProfile === 'gaming', admin: true, reboot: true },
          { key: 'no_power_throttling', on: activeProfile === 'gaming', admin: true, reboot: true },
          { key: 'no_gamebar', on: activeProfile === 'gaming', admin: false, reboot: false },
          { key: 'no_web_search', on: activeProfile === 'work', admin: false, reboot: false },
          { key: 'no_suggestions', on: activeProfile === 'work', admin: false, reboot: false },
          { key: 'no_mpo', on: true, admin: true, reboot: true },
          { key: 'end_task', on: true, admin: false, reboot: false },
          { key: 'no_activity', on: false, admin: true, reboot: false },
          { key: 'no_update_sharing', on: false, admin: true, reboot: false },
        ],
        has_backup: activeProfile !== 'balanced',
        memory: { total: 32 * GB, used: 17.4 * GB }, cpu: 23,
      };
      case 'sys_stats': return { memory: { total: 32 * GB, used: (16 + rnd() * 3) * GB }, cpu: 12 + rnd() * 30 };
      case 'opt_set_power_plan': return null;
      case 'opt_apply_profile': activeProfile = a.profile; return { applied: 6, failed: [], reboot: a.profile === 'gaming' };
      case 'opt_set_tweak': return { applied: 1, failed: [], reboot: false };
      case 'opt_restore': activeProfile = 'balanced'; return { applied: 7, failed: [], reboot: false };
      case 'opt_startup_list': return [
        { id: 'hkcu:Discord', name: 'Discord', command: 'C:\\Users\\alex\\AppData\\Local\\Discord\\Update.exe --processStart Discord.exe', enabled: true, scope: 'user' },
        { id: 'hkcu:Spotify', name: 'Spotify', command: 'C:\\Users\\alex\\AppData\\Roaming\\Spotify\\Spotify.exe /minimized', enabled: true, scope: 'user' },
        { id: 'hkcu:Steam', name: 'Steam', command: 'C:\\Program Files (x86)\\Steam\\steam.exe -silent', enabled: false, scope: 'user' },
        { id: 'hklm:SecurityHealth', name: 'SecurityHealth', command: '%windir%\\system32\\SecurityHealthSystray.exe', enabled: true, scope: 'machine' },
        { id: 'hklm:RtkAudUService', name: 'RtkAudUService', command: 'C:\\Windows\\System32\\RtkAudUService64.exe -background', enabled: true, scope: 'machine' },
      ];
      case 'opt_startup_set': return null;
      case 'opt_processes': return [
        { pids: [4120, 4121, 4122], name: 'chrome.exe', memory: 3.1 * GB, cpu: 4.2, group: 'browser' },
        { pids: [6620], name: 'ms-teams.exe', memory: 0.9 * GB, cpu: 1.1, group: 'chat' },
        { pids: [9932, 9933], name: 'Discord.exe', memory: 0.6 * GB, cpu: 0.4, group: 'chat' },
        { pids: [5501], name: 'EpicGamesLauncher.exe', memory: 0.35 * GB, cpu: 0.8, group: 'launcher' },
        { pids: [8800], name: 'Spotify.exe', memory: 0.32 * GB, cpu: 0.6, group: 'media' },
        { pids: [7710], name: 'OneDrive.exe', memory: 0.21 * GB, cpu: 0.1, group: 'sync' },
        { pids: [3300], name: 'vmmemWSL', memory: 4.2 * GB, cpu: 0.9, group: 'dev' },
        { pids: [3310], name: 'Docker Desktop.exe', memory: 0.4 * GB, cpu: 0.3, group: 'dev' },
      ];
      case 'opt_kill': return { applied: (a.pids ?? []).length, failed: [], reboot: false };
      case 'set_live_watch': case 'cancel_scan': case 'cancel_duplicates': case 'reveal': return null;
      case 'delete_nodes': return { freed: (a.ids as number[]).reduce((s, id) => s + nodes[id].size, 0), deleted: a.ids.length, failed: [] };
      case 'hardlink_dupes': return { freed: 12 * GB, deleted: a.pairs.length, failed: [] };
      case 'plugin:updater|check': return null;
      case 'plugin:dialog|open': return null;
      default:
        if (cmd.startsWith('plugin:')) return null;
        console.warn('[mock] unhandled command', cmd, a);
        return null;
    }
  }, { shouldMockEvents: true });
}
